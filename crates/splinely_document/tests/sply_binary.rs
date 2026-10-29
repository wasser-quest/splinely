//! Binary `.sply` codec: round trips over every example, hostile-input table,
//! deterministic mutation fuzzing, golden fixtures shared with the JS decoder.
use serde_json::{json, Value};
use splinely_document::{crc32c, decode_to_value, encode_value, Project, MAX_PROJECT_FILE_BYTES};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sply")
}

/// Equality that also distinguishes integer and float token kinds and -0.0.
fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            x.is_f64() == y.is_f64()
                && if x.is_f64() {
                    x.as_f64().unwrap().to_bits() == y.as_f64().unwrap().to_bits()
                } else {
                    x == y
                }
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| strict_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| strict_eq(v, w)))
        }
        _ => a == b,
    }
}

fn example_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root().join("examples"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json" || e == "sply"))
        .collect();
    files.sort();
    files
}

// ------------------------------------------------------------ raw builder

/// Build a file from raw string-table entries and a raw body; header sizes,
/// offsets and CRC are correct, so tests can corrupt exactly one rule.
fn build(strings: &[&[u8]], body: &[u8]) -> Vec<u8> {
    let data: Vec<u8> = strings.concat();
    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'S', b'P', b'L', b'Y', 0x0d, 0x0a, 0x1a]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    let mut offset = 0u32;
    out.extend_from_slice(&offset.to_le_bytes());
    for s in strings {
        offset += s.len() as u32;
        out.extend_from_slice(&offset.to_le_bytes());
    }
    out.extend_from_slice(&data);
    out.extend_from_slice(body);
    fix_crc(&mut out);
    out
}
fn fix_crc(bytes: &mut [u8]) {
    if bytes.len() >= 32 {
        let crc = crc32c(&bytes[32..]);
        bytes[28..32].copy_from_slice(&crc.to_le_bytes());
    }
}
fn varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
    out
}
fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}
fn err_of(bytes: &[u8]) -> String {
    decode_to_value(bytes).expect_err("must be rejected").0
}

// ------------------------------------------------------------ fixtures

struct Valid {
    name: &'static str,
    description: &'static str,
    bytes: Vec<u8>,
    expected: Value,
    lossy: bool,
}
struct Malformed {
    name: &'static str,
    rule: &'static str,
    error: &'static str,
    bytes: Vec<u8>,
}

fn nested_arrays(depth: usize) -> Vec<u8> {
    // `depth` nested single-element arrays, innermost is an empty array.
    let mut body = Vec::new();
    for _ in 0..depth - 1 {
        body.extend_from_slice(&[7, 1]);
    }
    body.extend_from_slice(&[7, 0]);
    body
}

fn valid_fixtures() -> Vec<Valid> {
    let mut list = Vec::new();
    let mut add = |name, description, value: Value, lossy| {
        list.push(Valid {
            name,
            description,
            bytes: encode_value(&value).unwrap(),
            expected: value,
            lossy,
        });
    };
    add(
        "scalars",
        "null/true/false/array root",
        json!([null, false, true]),
        false,
    );
    add("empty-object", "root is an empty object", json!({}), false);
    add(
        "int-float-kinds",
        "integers keep kind; 1 vs 1.0 vs -0.0; u64 max and i64 min",
        serde_json::from_str(
            r#"{"zero":0,"one":1,"onef":1.0,"negzero":-0.0,"neg":-1,"u64max":18446744073709551615,"i64min":-9223372036854775808,"i64max":9223372036854775807,"exp":1.5e-7,"big":1e300}"#,
        )
        .unwrap(),
        true,
    );
    add(
        "strings-dedupe",
        "unicode strings, empty string/key, repeated strings share one table entry",
        json!({"a": "héllo ✓ 😀", "b": "héllo ✓ 😀", "": "", "k": ["héllo ✓ 😀", ""]}),
        false,
    );
    add(
        "float-arrays",
        "arrays of >=2 floats use tag 9; mixed/single/empty arrays use tag 7",
        serde_json::from_str(
            r#"{"points":[0.5,1.0,-2.5e300,5e-324],"mixed":[1.0,2],"one":[1.5],"empty":[],"ints":[1,2,3],"nested":[[0.0,-0.0],[1.25,2.5]]}"#,
        )
        .unwrap(),
        false,
    );
    add(
        "audio-blob",
        "dataBase64 canonical padded base64 uses tag 10; same text elsewhere stays a string",
        json!({"assets":[
            {"id":"a","dataBase64":"AAECAwQ="},
            {"id":"b","dataBase64":"not base64!"},
            {"id":"c","dataBase64":""},
            {"id":"d","dataBase64":"Zm9v"},
            {"id":"e","dataBase64":"Zg=="}],
          "other":"AAECAwQ="}),
        false,
    );
    add(
        "nesting-128",
        "128 nested arrays (the maximum depth)",
        {
            let mut v = json!([]);
            for _ in 0..127 {
                v = json!([v]);
            }
            v
        },
        false,
    );
    add(
        "dense-nulls",
        "3000 nulls in one array: the densest legal body (1 byte per item) must stay within the claim budget",
        Value::Array(vec![Value::Null; 3000]),
        false,
    );
    {
        // 127 nested arrays around a tag-9 float array: tag 9 at depth 128 is legal.
        let mut expected = json!([1.0, -2.5]);
        for _ in 0..127 {
            expected = json!([expected]);
        }
        let mut body = Vec::new();
        for _ in 0..127 {
            body.extend_from_slice(&[7, 1]);
        }
        body.extend_from_slice(&cat(&[
            &[9, 2],
            &1.0f64.to_le_bytes(),
            &(-2.5f64).to_le_bytes(),
        ]));
        list.push(Valid {
            name: "floats-depth-128",
            description: "tag 9 float array nested at the maximum depth 128",
            bytes: build(&[], &body),
            expected,
            lossy: false,
        });
    }
    list.push(Valid {
        name: "minimal-project",
        description: "Project::default() encoded by the Rust encoder",
        bytes: Project::default().to_binary().unwrap(),
        expected: serde_json::to_value(Project::default()).unwrap(),
        lossy: false,
    });
    // Hand-built, non-canonical but legal encodings the decoder must accept.
    let f1 = 1.0f64.to_le_bytes();
    let f2 = (-2.5f64).to_le_bytes();
    let mut body = vec![8, 6]; // object with 6 members
    for (key, value) in [
        (0u8, cat(&[&[7, 2, 5], &f1, &[5], &f2])), // plain array of tag-5 floats
        (1, cat(&[&[9, 2], &f1, &f2])),            // tag 9 (also valid for floats)
        (2, cat(&[&[9, 0]])),                      // empty tag-9 array
        (3, cat(&[&[9, 1], &f1])),                 // single-element tag-9 array
        (4, cat(&[&[10, 3, b'f', b'o', b'o']])),   // blob anywhere a string is valid
        (5, cat(&[&[10, 0]])),                     // empty blob == ""
    ] {
        body.push(key);
        body.extend_from_slice(&value);
    }
    list.push(Valid {
        name: "noncanonical-accepted",
        description: "tag 7 float arrays, tag 9 single/empty, tag 10 on any key and empty; unused table string",
        bytes: build(&[b"plain", b"packed", b"empty", b"single", b"blob", b"emptyblob", b"unused"], &body),
        expected: json!({"plain":[1.0,-2.5],"packed":[1.0,-2.5],"empty":[],"single":[1.0],"blob":"Zm9v","emptyblob":""}),
        lossy: false,
    });
    list
}

fn malformed_fixtures() -> Vec<Malformed> {
    let mut list = Vec::new();
    let mut add = |name, rule, error, bytes: Vec<u8>| {
        list.push(Malformed {
            name,
            rule,
            error,
            bytes,
        })
    };
    let ok = build(&[b"a"], &[8, 1, 0, 0]); // {"a":null}
    assert_eq!(decode_to_value(&ok).unwrap(), json!({"a": null}));
    let with = |f: &dyn Fn(&mut Vec<u8>), crc: bool| {
        let mut b = ok.clone();
        f(&mut b);
        if crc {
            fix_crc(&mut b);
        }
        b
    };
    add("empty-input", "wrong magic", "bad magic", vec![]);
    add(
        "bad-magic",
        "wrong magic",
        "bad magic",
        with(&|b| b[1] = b'X', true),
    );
    add(
        "short-header",
        "truncated header",
        "truncated header",
        ok[..20].to_vec(),
    );
    add(
        "major-2",
        "wrong major",
        "unsupported major version",
        with(&|b| b[8] = 2, true),
    );
    add(
        "major-0",
        "wrong major",
        "unsupported major version",
        with(&|b| b[8] = 0, true),
    );
    add(
        "minor-1",
        "minor higher than supported",
        "unsupported minor version",
        with(&|b| b[10] = 1, true),
    );
    add(
        "flags-set",
        "flag bit set",
        "unsupported flags",
        with(&|b| b[12] = 1, true),
    );
    add(
        "flags-high-bit",
        "flag bit set",
        "unsupported flags",
        with(&|b| b[15] = 0x80, true),
    );
    add(
        "trailing-byte",
        "size mismatch (trailing bytes)",
        "size mismatch",
        with(&|b| b.push(0), true),
    );
    add(
        "truncated-file",
        "size mismatch (truncated)",
        "size mismatch",
        ok[..ok.len() - 1].to_vec(),
    );
    add(
        "huge-string-count",
        "string_count larger than file (no allocation)",
        "size mismatch",
        with(
            &|b| b[16..20].copy_from_slice(&u32::MAX.to_le_bytes()),
            true,
        ),
    );
    add(
        "huge-body-len",
        "body_len larger than file",
        "size mismatch",
        with(
            &|b| b[24..28].copy_from_slice(&u32::MAX.to_le_bytes()),
            true,
        ),
    );
    add(
        "crc-flipped",
        "crc mismatch",
        "crc mismatch",
        with(&|b| b[28] ^= 1, false),
    );
    add("body-bit-flip", "crc mismatch", "crc mismatch", {
        let n = ok.len();
        with(&|b| b[n - 1] ^= 0x10, false)
    });
    // ok layout: 32 header, offsets @32..40 (N=1 -> 2 offsets), string @40, body @41..
    add(
        "offset0-nonzero",
        "offsets[0] != 0",
        "invalid string offsets",
        with(&|b| b[32] = 1, true),
    );
    add(
        "offset-last-mismatch",
        "offsets[N] != strings_len",
        "invalid string offsets",
        with(&|b| b[36] = 2, true),
    );
    {
        let mut b = build(&[b"ab", b"c"], &[8, 1, 0, 0]);
        // offsets: 0,2,3 -> make them 0,3,3 + string lens 3/0 => still monotonic and valid; instead break order
        b[36..40].copy_from_slice(&5u32.to_le_bytes()); // offsets 0,5,3
        fix_crc(&mut b);
        add(
            "offsets-decreasing",
            "offsets non-monotonic / out of range",
            "invalid string offsets",
            b,
        );
    }
    add(
        "utf8-invalid",
        "invalid UTF-8 in string table",
        "invalid utf-8",
        build(&[&[0xff, 0xfe]], &[0]),
    );
    add(
        "utf8-truncated-seq",
        "invalid UTF-8 (cut multibyte)",
        "invalid utf-8",
        build(&[&[0xe2, 0x82]], &[0]),
    );
    add(
        "utf8-surrogate",
        "invalid UTF-8 (encoded surrogate)",
        "invalid utf-8",
        build(&[&[0xed, 0xa0, 0x80]], &[0]),
    );
    add(
        "duplicate-string",
        "duplicate string table entries",
        "duplicate string",
        build(&[b"a", b"a"], &[0]),
    );
    add(
        "duplicate-empty-string",
        "duplicate string table entries",
        "duplicate string",
        build(&[b"", b""], &[0]),
    );
    add(
        "duplicate-key",
        "duplicate object key",
        "duplicate object key",
        build(&[b"a"], &[8, 2, 0, 0, 0, 0]),
    );
    add(
        "unknown-tag-11",
        "unknown tag",
        "unknown tag",
        build(&[], &[11]),
    );
    add(
        "unknown-tag-255",
        "unknown tag",
        "unknown tag",
        build(&[], &[255]),
    );
    add(
        "nonminimal-varint",
        "non-minimal varint",
        "non-minimal varint",
        build(&[], &[3, 0x80, 0x00]),
    );
    add(
        "nonminimal-varint-long",
        "non-minimal varint",
        "non-minimal varint",
        build(&[], &[3, 0x81, 0x80, 0x00]),
    );
    add(
        "varint-overflow-10th-byte",
        "varint overflow",
        "varint overflow",
        build(
            &[],
            &[
                3, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02,
            ],
        ),
    );
    add(
        "varint-too-long",
        "varint longer than 10 bytes",
        "varint overflow",
        build(
            &[],
            &[
                3, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01,
            ],
        ),
    );
    add(
        "varint-truncated",
        "varint cut by end of body",
        "unexpected end of body",
        build(&[], &[3, 0x80]),
    );
    add(
        "negint-out-of-range",
        "negative integer below -2^63",
        "negative integer out of range",
        build(&[], &cat(&[&[4], &varint(1 << 63)])),
    );
    add(
        "float-nan",
        "non-finite float (NaN)",
        "non-finite float",
        build(&[], &cat(&[&[5], &f64::NAN.to_le_bytes()])),
    );
    add(
        "float-inf",
        "non-finite float (+inf)",
        "non-finite float",
        build(&[], &cat(&[&[5], &f64::INFINITY.to_le_bytes()])),
    );
    add(
        "float-neg-inf-in-array",
        "non-finite float inside tag 9",
        "non-finite float",
        build(
            &[],
            &cat(&[
                &[9, 2],
                &1.0f64.to_le_bytes(),
                &f64::NEG_INFINITY.to_le_bytes(),
            ]),
        ),
    );
    add(
        "float-truncated",
        "tag 5 cut short",
        "unexpected end of body",
        build(&[], &[5, 0, 0, 0, 0]),
    );
    add(
        "depth-129",
        "nesting deeper than 128",
        "nesting too deep",
        build(&[], &nested_arrays(129)),
    );
    add(
        "depth-1000",
        "nesting deeper than 128",
        "nesting too deep",
        build(&[], &nested_arrays(1000)),
    );
    add(
        "depth-129-object",
        "nesting deeper than 128 through objects",
        "nesting too deep",
        {
            let mut body = Vec::new();
            for _ in 0..128 {
                body.extend_from_slice(&[8, 1, 0]);
            }
            body.extend_from_slice(&[8, 0]);
            build(&[b"k"], &body)
        },
    );
    {
        // 128 nested arrays, then a tag-9 array at depth 129.
        let mut body = Vec::new();
        for _ in 0..128 {
            body.extend_from_slice(&[7, 1]);
        }
        body.extend_from_slice(&cat(&[
            &[9, 2],
            &1.0f64.to_le_bytes(),
            &2.0f64.to_le_bytes(),
        ]));
        add(
            "floats-depth-129",
            "tag 9 float array nested deeper than 128",
            "nesting too deep",
            build(&[], &body),
        );
    }
    {
        // Three levels each claim 30 items of a 40-byte body: every single count fits
        // the remaining bytes, but the claims sum to more than the body.
        let mut body = vec![7, 30, 7, 30, 7, 30];
        body.resize(40, 0);
        add(
            "claims-exceed-body",
            "sum of container claims exceeds the body length",
            "count exceeds remaining bytes",
            build(&[], &body),
        );
    }
    add(
        "negint-max-varint",
        "negative integer with varint 2^64-1 (below -2^63)",
        "negative integer out of range",
        build(&[], &cat(&[&[4], &varint(u64::MAX)])),
    );
    add(
        "array-count-too-big",
        "array count > remaining bytes",
        "count exceeds remaining bytes",
        build(&[], &[7, 5, 0, 0]),
    );
    add(
        "array-count-huge",
        "array count of 2^63 elements",
        "count exceeds remaining bytes",
        build(&[], &cat(&[&[7], &varint(1 << 63)])),
    );
    add(
        "object-count-too-big",
        "object count*2 > remaining",
        "count exceeds remaining bytes",
        build(&[b"a"], &[8, 3, 0, 0, 0]),
    );
    add(
        "floats-count-too-big",
        "tag 9 count*8 > remaining",
        "count exceeds remaining bytes",
        build(&[], &cat(&[&[9, 2], &1.0f64.to_le_bytes()])),
    );
    add(
        "floats-count-overflow",
        "tag 9 count*8 overflows u64",
        "count exceeds remaining bytes",
        build(&[], &cat(&[&[9], &varint(u64::MAX)])),
    );
    add(
        "blob-length-too-big",
        "tag 10 length > remaining",
        "unexpected end of body",
        build(&[], &[10, 4, 1, 2, 3]),
    );
    add(
        "blob-length-huge",
        "tag 10 length 2^63",
        "unexpected end of body",
        build(&[], &cat(&[&[10], &varint(1 << 63)])),
    );
    add(
        "trailing-body-bytes",
        "bytes left after the root value",
        "trailing bytes in body",
        build(&[], &[0, 0]),
    );
    add(
        "empty-body",
        "no root value",
        "unexpected end of body",
        build(&[], &[]),
    );
    add(
        "string-index-out-of-range",
        "string index >= string_count",
        "invalid string index",
        build(&[b"a"], &[6, 1]),
    );
    add(
        "string-index-no-table",
        "string index with empty table",
        "invalid string index",
        build(&[], &[6, 0]),
    );
    add(
        "string-index-huge",
        "string index 2^63",
        "invalid string index",
        build(&[b"a"], &cat(&[&[6], &varint(1 << 63)])),
    );
    add(
        "key-index-out-of-range",
        "object key index >= string_count",
        "invalid string index",
        build(&[b"a"], &[8, 1, 7, 0]),
    );
    {
        // One 100 KB string referenced 3000 times: 6 KB of body would expand to 300 MB.
        let big = vec![b'x'; 100_000];
        let mut body = vec![7];
        body.extend_from_slice(&varint(3000));
        for _ in 0..3000 {
            body.extend_from_slice(&[6, 0]);
        }
        add(
            "string-amplification",
            "reference amplification of one large string",
            "string expansion limit exceeded",
            build(&[&big], &body),
        );
    }
    list
}

fn encode_manifest_text(valid: &[Valid], malformed: &[Malformed]) -> String {
    let valid: Vec<Value> = valid
        .iter()
        .map(|v| {
            json!({
                "file": format!("valid/{}.sply", v.name),
                "description": v.description,
                "expectedText": serde_json::to_string(&v.expected).unwrap(),
                "lossy": v.lossy,
            })
        })
        .collect();
    let malformed: Vec<Value> = malformed
        .iter()
        .map(|m| {
            json!({
                "file": format!("malformed/{}.sply", m.name),
                "rule": m.rule,
                "error": m.error,
            })
        })
        .collect();
    let manifest = json!({
        "format": "sply-binary",
        "version": "1.0",
        "notes": "valid[].expectedText is the exact JSON text of the decoded tree (1 vs 1.0 distinguishes int/float kinds; lossy=true means it holds numbers a JS Number cannot represent). malformed[].error is a substring of the Rust decoder error (prefix 'sply: '); the JS and Python decoders must reject every malformed file with a 'sply: ' error (wording may differ; all three decoders enforce the same limits, including string expansion, container claims and tag-9 depth).",
        "valid": valid,
        "malformed": malformed,
    });
    serde_json::to_string_pretty(&manifest).unwrap() + "\n"
}

#[test]
fn golden_fixtures_are_current_and_decode_as_declared() {
    let valid = valid_fixtures();
    let malformed = malformed_fixtures();
    assert!(valid.len() >= 6);
    let regenerate = std::env::var_os("SPLY_REGEN_FIXTURES").is_some();
    let mut expected_files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for v in &valid {
        expected_files.push((
            fixtures().join(format!("valid/{}.sply", v.name)),
            v.bytes.clone(),
        ));
    }
    for m in &malformed {
        expected_files.push((
            fixtures().join(format!("malformed/{}.sply", m.name)),
            m.bytes.clone(),
        ));
    }
    expected_files.push((
        fixtures().join("manifest.json"),
        encode_manifest_text(&valid, &malformed).into_bytes(),
    ));
    if regenerate {
        for (path, bytes) in &expected_files {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }
    for (path, bytes) in &expected_files {
        let on_disk = std::fs::read(path)
            .unwrap_or_else(|e| panic!("{}: {e} (run with SPLY_REGEN_FIXTURES=1)", path.display()));
        assert_eq!(
            &on_disk,
            bytes,
            "{} is stale (SPLY_REGEN_FIXTURES=1)",
            path.display()
        );
    }
    for v in &valid {
        let decoded = decode_to_value(&v.bytes).unwrap_or_else(|e| panic!("{}: {e}", v.name));
        assert!(
            strict_eq(&decoded, &v.expected),
            "{}: {decoded} != {}",
            v.name,
            v.expected
        );
        // The manifest text round-trips to the same tree (kinds included).
        // (serde_json itself refuses to parse >127 nested levels.)
        if !v.lossy && v.name != "nesting-128" && v.name != "floats-depth-128" {
            let parsed: Value =
                serde_json::from_str(&serde_json::to_string(&v.expected).unwrap()).unwrap();
            assert!(strict_eq(&parsed, &decoded), "{}", v.name);
        }
    }
    for m in &malformed {
        let message = err_of(&m.bytes);
        assert!(
            message.contains(m.error),
            "{}: {message:?} lacks {:?}",
            m.name,
            m.error
        );
        assert!(Project::from_binary(&m.bytes).is_err(), "{}", m.name);
    }
}

#[test]
fn canonical_encoder_uses_packed_tags_and_blobs() {
    let value =
        json!({"assets":[{"id":"a","dataBase64":"AAECAwQ="}],"other":"AAECAwQ=","p":[0.5,1.5]});
    let bytes = encode_value(&value).unwrap();
    // blob bytes appear raw exactly once, as the text only for the plain string field
    assert!(bytes.windows(5).any(|w| w == [0, 1, 2, 3, 4]));
    assert_eq!(bytes.windows(8).filter(|w| *w == b"AAECAwQ=").count(), 1);
    assert!(bytes.windows(2 + 16).any(|w| w[0] == 9 && w[1] == 2));
    assert!(strict_eq(&decode_to_value(&bytes).unwrap(), &value));
    // deterministic
    assert_eq!(bytes, encode_value(&value).unwrap());
    // depth limit symmetric in the encoder
    let mut deep = json!([]);
    for _ in 0..128 {
        deep = json!([deep]);
    }
    assert!(encode_value(&deep).is_err());
}

#[test]
fn depth_boundary_is_128() {
    assert!(decode_to_value(&build(&[], &nested_arrays(128))).is_ok());
    assert!(err_of(&build(&[], &nested_arrays(129))).contains("nesting too deep"));
}

// ------------------------------------------------------------ round trips

#[test]
fn every_example_round_trips_through_binary() {
    let mut checked = 0;
    for path in example_files() {
        let raw_bytes = std::fs::read(&path).unwrap();
        // Examples may be committed as binary `.sply`: use their canonical JSON text.
        let text = if Project::is_binary(&raw_bytes) {
            Project::from_bytes(&raw_bytes).unwrap().to_json().unwrap()
        } else {
            String::from_utf8(raw_bytes).unwrap()
        };
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // Raw JSON tree (no project semantics): exact kind-preserving round trip.
        let raw: Value = serde_json::from_str(&text).unwrap();
        let bytes = encode_value(&raw).unwrap();
        let decoded = decode_to_value(&bytes).unwrap();
        assert!(strict_eq(&raw, &decoded), "{name}: raw tree changed");
        assert_eq!(
            encode_value(&decoded).unwrap(),
            bytes,
            "{name}: not deterministic"
        );
        // Project semantics.
        let project = Project::from_json(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let binary = project.to_binary().unwrap();
        assert!(Project::is_binary(&binary) && !Project::is_binary(text.as_bytes()));
        let canonical_tree = serde_json::to_value(&project).unwrap();
        assert!(
            strict_eq(&decode_to_value(&binary).unwrap(), &canonical_tree),
            "{name}"
        );
        // binary Value == JSON Value of the canonical text
        let canonical_text: Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
        assert!(
            strict_eq(&decode_to_value(&binary).unwrap(), &canonical_text),
            "{name}"
        );
        assert_eq!(Project::from_binary(&binary).unwrap(), project, "{name}");
        assert_eq!(Project::from_bytes(&binary).unwrap(), project, "{name}");
        assert_eq!(
            Project::from_bytes(text.as_bytes()).unwrap(),
            project,
            "{name}"
        );
        // Binary built from the *authored* tree loads exactly like the authored text.
        assert_eq!(
            Project::from_binary(&bytes).unwrap(),
            project,
            "{name}: authored tree"
        );
        assert!(binary.len() < text.len(), "{name}: binary not smaller");
        checked += 1;
    }
    assert!(checked >= 12, "only {checked} example files checked");
}

#[test]
fn binary_loading_is_as_strict_as_json() {
    let base = serde_json::to_value(Project::default()).unwrap();
    let load = |v: &Value| Project::from_binary(&encode_value(v).unwrap());
    assert!(load(&base).is_ok());
    // unknown top-level and nested fields
    let mut unknown = base.clone();
    unknown["bogus"] = json!(true);
    assert!(load(&unknown).is_err());
    assert!(Project::from_json(&unknown.to_string()).is_err());
    let mut nested = base.clone();
    nested["nodes"] = json!([{"id":"a","name":"A","kind":{"type":"circle","cx":0,"cy":0,"radius":10,"unsupported":true}}]);
    assert!(load(&nested).is_err());
    // lossy integer -> float (2^53 + 1) must be refused, exactly like JSON text
    let mut lossy = base.clone();
    lossy["width"] = json!(9007199254740993u64);
    assert!(load(&lossy).is_err());
    assert!(Project::from_json(&lossy.to_string()).is_err());
    // exact integer where a float is expected is accepted by both
    let mut exact = base.clone();
    exact["width"] = json!(640);
    assert_eq!(
        load(&exact).unwrap(),
        Project::from_json(&exact.to_string()).unwrap()
    );
    // invalid project values fail validate()
    let mut invalid = base.clone();
    invalid["width"] = json!(-1.0);
    assert!(load(&invalid).is_err());
    // wrong types
    let mut typed = base.clone();
    typed["name"] = json!(5);
    assert!(load(&typed).is_err());
    assert!(load(&json!([1, 2])).is_err());
    // legacy normalisation applies to binary too (explicit default fields)
    let mut legacy = base.clone();
    legacy["followers"] = json!([]);
    legacy["speechCues"] = json!([]);
    assert_eq!(
        load(&legacy).unwrap(),
        Project::from_json(&legacy.to_string()).unwrap()
    );
    // from_bytes: bad UTF-8 / garbage are errors, not panics
    assert!(Project::from_bytes(&[0xff, 0xfe, 0x00]).is_err());
    assert!(Project::from_bytes(b"").is_err());
    assert!(Project::from_bytes(b"\x89SPLY").is_err());
}

// ------------------------------------------------------------ fuzz-ish

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[test]
fn mutated_and_truncated_inputs_never_panic() {
    let demo = std::fs::read_to_string(root().join("examples/demo.splinely.json")).unwrap();
    let project = Project::from_json(&demo).unwrap();
    let mut seeds: Vec<Vec<u8>> = vec![project.to_binary().unwrap()];
    seeds.extend(valid_fixtures().into_iter().map(|v| v.bytes));
    seeds.extend(malformed_fixtures().into_iter().map(|m| m.bytes));
    let mut rng = Rng(0x5eed_1234_abcd_ef01);
    let (mut accepted, mut rejected) = (0u32, 0u32);
    for iteration in 0..20_000u32 {
        let mut bytes = seeds[rng.below(seeds.len())].clone();
        for _ in 0..1 + rng.below(3) {
            match rng.below(6) {
                0 if !bytes.is_empty() => {
                    let cut = rng.below(bytes.len());
                    bytes.truncate(cut);
                }
                1 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes[at] ^= 1 << rng.below(8);
                }
                2 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes[at] = rng.next() as u8;
                }
                3 => {
                    let at = rng.below(bytes.len() + 1);
                    bytes.insert(at, rng.next() as u8);
                }
                4 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes.remove(at);
                }
                5 if bytes.len() > 40 => {
                    // overwrite a 4-byte word with an extreme value (counts/offsets)
                    let at = rng.below(bytes.len() - 4);
                    let word = [0u32, 1, 0x7fff_ffff, 0xffff_ffff, 0x8000_0000][rng.below(5)];
                    bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
                }
                _ => {}
            }
        }
        // Half the time make the CRC valid so the parser sees the corruption.
        if rng.below(2) == 0 {
            fix_crc(&mut bytes);
            // keep header sizes consistent with the mutated length sometimes
            if bytes.len() >= 32 && rng.below(2) == 0 {
                let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
                let offsets = 4usize.saturating_mul(count.saturating_add(1));
                if let Some(data_start) = 32usize.checked_add(offsets).filter(|s| *s <= bytes.len())
                {
                    let strings_len =
                        u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
                    if let Some(body_start) = data_start
                        .checked_add(strings_len)
                        .filter(|s| *s <= bytes.len())
                    {
                        let body_len = (bytes.len() - body_start) as u32;
                        bytes[24..28].copy_from_slice(&body_len.to_le_bytes());
                        fix_crc(&mut bytes);
                    }
                }
            }
        }
        match decode_to_value(&bytes) {
            Ok(_) => accepted += 1,
            Err(_) => rejected += 1,
        }
        if iteration % 8 == 0 {
            let _ = Project::from_binary(&bytes);
            let _ = Project::from_bytes(&bytes);
        }
    }
    eprintln!("mutation loop: accepted={accepted} rejected={rejected}");
    assert!(rejected > 10_000);
    // Pure random garbage with a valid magic prefix.
    for _ in 0..2000 {
        let mut bytes = vec![0x89, b'S', b'P', b'L', b'Y', 0x0d, 0x0a, 0x1a];
        for _ in 0..rng.below(200) {
            bytes.push(rng.next() as u8);
        }
        let _ = decode_to_value(&bytes);
    }
}

#[test]
fn exhaustive_single_bit_flips_of_a_small_file() {
    let bytes = encode_value(&json!({"a":[0.5,1.5,{"b":"c"}],"d":-3,"e":null})).unwrap();
    for at in 0..bytes.len() {
        for bit in 0..8 {
            let mut flipped = bytes.clone();
            flipped[at] ^= 1 << bit;
            let _ = decode_to_value(&flipped);
            fix_crc(&mut flipped);
            let _ = decode_to_value(&flipped);
        }
    }
    for cut in 0..bytes.len() {
        assert!(decode_to_value(&bytes[..cut]).is_err());
    }
}

// ------------------------------------------------------------ files

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("splinely-sply-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn file_api_sniffs_binary_and_keeps_json() {
    let dir = scratch("files");
    let text = std::fs::read_to_string(root().join("examples/demo.splinely.json")).unwrap();
    let project = Project::from_json(&text).unwrap();
    let bin = dir.join("a.sply");
    let json_path = dir.join("b.sply");
    project.save_file_binary(&bin).unwrap();
    project.save_file(&json_path).unwrap();
    assert!(Project::is_binary(&std::fs::read(&bin).unwrap()));
    assert!(std::fs::read_to_string(&json_path)
        .unwrap()
        .starts_with('{'));
    assert_eq!(Project::load_file(&bin).unwrap(), project);
    assert_eq!(Project::load_file(&json_path).unwrap(), project);
    // binary file in place stays untouched after a failed binary save
    let before = std::fs::read(&bin).unwrap();
    let mut invalid = project.clone();
    invalid.width = -1.0;
    assert!(invalid.save_file_binary(&bin).is_err());
    assert!(project
        .save_file_binary(dir.join("missing/x.sply"))
        .is_err());
    assert_eq!(std::fs::read(&bin).unwrap(), before);
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        2,
        "no temporaries left"
    );
    // corrupt binary reports the path and does not panic
    let mut corrupt = before.clone();
    corrupt[40] ^= 1;
    std::fs::write(&bin, &corrupt).unwrap();
    let error = Project::load_file(&bin).unwrap_err().0;
    assert!(
        error.contains("a.sply") && error.contains("crc mismatch"),
        "{error}"
    );
    // 64 MiB cap applies to binary too
    let huge = dir.join("huge.sply");
    let mut header = vec![0x89, b'S', b'P', b'L', b'Y', 0x0d, 0x0a, 0x1a];
    header.resize(32, 0);
    std::fs::write(&huge, &header).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&huge)
        .unwrap()
        .set_len(MAX_PROJECT_FILE_BYTES + 1)
        .unwrap();
    assert!(Project::load_file(&huge).unwrap_err().0.contains("64 MiB"));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `cargo test --release -p splinely_document --test sply_binary -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_desert_stickfight() {
    let path = root().join("examples/desert-stickfight.splinely.json");
    let text = std::fs::read_to_string(path).unwrap();
    let project = Project::from_json(&text).unwrap();
    let binary = project.to_binary().unwrap();
    let canonical = project.to_json().unwrap();
    fn best(n: usize, mut f: impl FnMut()) -> f64 {
        (0..n)
            .map(|_| {
                let start = Instant::now();
                f();
                start.elapsed().as_secs_f64() * 1e3
            })
            .fold(f64::MAX, f64::min)
    }
    let json_ms = best(15, || drop(Project::from_json(&text).unwrap()));
    let bin_ms = best(15, || drop(Project::from_binary(&binary).unwrap()));
    let decode_ms = best(15, || drop(decode_to_value(&binary).unwrap()));
    let parse_ms = best(15, || drop(serde_json::from_str::<Value>(&text).unwrap()));
    let encode_ms = best(15, || drop(project.to_binary().unwrap()));
    let crc_ms = best(15, || {
        std::hint::black_box(crc32c(&binary));
    });
    println!(
        "desert-stickfight: json {} B (canonical {} B) -> binary {} B ({:.1}%)",
        text.len(),
        canonical.len(),
        binary.len(),
        100.0 * binary.len() as f64 / text.len() as f64
    );
    println!(
        "load  from_json {json_ms:.2} ms | from_binary {bin_ms:.2} ms | speedup {:.2}x",
        json_ms / bin_ms
    );
    println!("parts serde Value parse {parse_ms:.2} ms | decode_to_value {decode_ms:.2} ms | crc32c {crc_ms:.3} ms | to_binary {encode_ms:.2} ms");
    let mut worst = 0f64;
    let mut worst_strings = 0f64;
    for path in example_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        let bin = Project::from_json(&text).unwrap().to_binary().unwrap();
        let tree = decode_to_value(&bin).unwrap();
        worst = worst.max(tree.to_string().len() as f64 / bin.len() as f64);
        fn strings(v: &Value) -> usize {
            match v {
                Value::String(s) => s.len(),
                Value::Array(a) => a.iter().map(strings).sum(),
                Value::Object(o) => o.iter().map(|(k, v)| k.len() + strings(v)).sum(),
                _ => 0,
            }
        }
        worst_strings = worst_strings.max(strings(&tree) as f64 / bin.len() as f64);
    }
    println!("max (compact JSON text size / binary size) over examples: {worst:.2}x");
    println!("max (decoded string bytes / binary size) over examples: {worst_strings:.2}x (decoder limit 16x)");
}

/// `cargo test --release -p splinely_document --test sply_binary bench_load_phases -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_load_phases() {
    fn best(n: usize, mut f: impl FnMut()) -> f64 {
        (0..n)
            .map(|_| {
                let start = Instant::now();
                f();
                start.elapsed().as_secs_f64() * 1e3
            })
            .fold(f64::MAX, f64::min)
    }
    for name in ["desert-stickfight", "stickfight", "demo"] {
        let text =
            std::fs::read_to_string(root().join(format!("examples/{name}.splinely.json"))).unwrap();
        let project = Project::from_json(&text).unwrap();
        let binary = project.to_binary().unwrap();
        let json_ms = best(15, || drop(Project::from_json(&text).unwrap()));
        let bin_ms = best(15, || drop(Project::from_binary(&binary).unwrap()));
        let validate_ms = best(15, || project.validate().unwrap());
        println!(
            "{name}: from_json {json_ms:.2} ms | from_binary {bin_ms:.2} ms | validate {validate_ms:.3} ms"
        );
    }
}

// ------------------------------------------------------------ differential

/// Load the same logical document through both front ends and require the
/// identical outcome: same `Project` (compared as bit-faithful JSON, so int/float
/// kinds and -0.0 count) or the same error message.
fn assert_same_outcome(bytes: &[u8], value: &Value, label: &str, stats: &mut [u32; 2]) {
    let binary = Project::from_binary(bytes);
    let json = Project::from_json(&value.to_string());
    match (binary, json) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a, b, "{label}: projects differ");
            assert_eq!(
                serde_json::to_string(&a).unwrap(),
                serde_json::to_string(&b).unwrap(),
                "{label}: projects differ bit-wise"
            );
            stats[0] += 1;
        }
        (Err(a), Err(b)) => {
            assert_eq!(a, b, "{label}: error messages differ");
            stats[1] += 1;
        }
        (a, b) => panic!(
            "{label}: outcome class differs: binary {:?} vs json {:?}",
            a.map(|_| "Ok"),
            b.map(|_| "Ok")
        ),
    }
}

fn example_values() -> Vec<(String, Value)> {
    example_files()
        .into_iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let bytes = std::fs::read(&path).unwrap();
            let value = if Project::is_binary(&bytes) {
                decode_to_value(&bytes).unwrap()
            } else {
                serde_json::from_slice(&bytes).unwrap()
            };
            (name, value)
        })
        .filter(|(_, v)| v.get("schemaVersion").is_some())
        .collect()
}

/// Random structural mutation somewhere in the tree: kind flips (int<->float),
/// extreme numbers, type swaps, unknown members, array edits, and the members
/// `normalize_project_value` rewrites (empty optionals, explicit defaults, speech).
fn mutate_value(v: &mut Value, rng: &mut Rng) {
    let descend = rng.below(5) != 0;
    match v {
        Value::Object(map) if descend && !map.is_empty() => {
            let at = rng.below(map.len());
            let key = map.keys().nth(at).unwrap().clone();
            return mutate_value(map.get_mut(&key).unwrap(), rng);
        }
        Value::Array(items) if descend && !items.is_empty() => {
            let at = rng.below(items.len());
            return mutate_value(&mut items[at], rng);
        }
        _ => {}
    }
    let bait: [(&str, Value); 12] = [
        ("onExit", json!([])),
        ("blend", json!(0)),
        ("blend", json!(0.0)),
        ("after", Value::Null),
        ("restAngle", json!(0)),
        ("origin", Value::Null),
        ("when", Value::Null),
        ("motionPath", Value::Null),
        ("speechCues", json!([{"id":"s"}])),
        ("followers", json!([])),
        ("groups", json!([])),
        ("guides", json!([])),
    ];
    match rng.below(14) {
        0 => match v {
            Value::Number(n) if n.is_f64() => {
                let f = n.as_f64().unwrap();
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    *v = json!(f as i64);
                }
            }
            Value::Number(n) => *v = json!(n.as_f64().unwrap()),
            _ => {}
        },
        1 => {
            if v.is_number() {
                *v = [
                    json!(u64::MAX),
                    json!(-1),
                    json!(1e300),
                    json!(9007199254740993u64),
                    json!(0.1),
                    json!(-0.0),
                    json!(i64::MIN),
                ][rng.below(7)]
                .clone();
            }
        }
        2 => *v = Value::Null,
        3 => *v = json!(true),
        4 => match v {
            Value::String(s) => s.push_str(["x", " ", "é", "/"][rng.below(4)]),
            _ => *v = json!("x"),
        },
        5 => *v = json!([]),
        6 => *v = json!({}),
        7 => {
            if let Value::Object(map) = v {
                map.insert("zz-unknown".into(), json!(1));
            }
        }
        8 => {
            if let Value::Object(map) = v {
                if !map.is_empty() {
                    let at = rng.below(map.len());
                    let key = map.keys().nth(at).unwrap().clone();
                    map.remove(&key);
                }
            }
        }
        9 => {
            if let Value::Array(items) = v {
                match rng.below(4) {
                    0 => {
                        items.pop();
                    }
                    1 => {
                        if let Some(first) = items.first().cloned() {
                            items.push(first);
                        }
                    }
                    2 => items.push(Value::Null),
                    _ => items.reverse(),
                }
            }
        }
        10..=12 => {
            if let Value::Object(map) = v {
                let (key, value) = bait[rng.below(bait.len())].clone();
                map.insert(key.into(), value);
            }
        }
        _ => {
            if let Value::Array(items) = v {
                items.push(json!({"type":"speak","text":"hi"}));
            }
        }
    }
}

#[test]
fn differential_binary_vs_json_on_mutated_examples() {
    let mut stats = [0u32; 2];
    let mut rng = Rng(0xd1ff_e4e7_7157_0001);
    for (name, value) in example_values() {
        let bytes = encode_value(&value).unwrap();
        assert_same_outcome(&bytes, &value, &name, &mut stats);
        let rounds = match bytes.len() {
            0..=100_000 => 120,
            100_001..=400_000 => 24,
            _ => 5,
        };
        for round in 0..rounds {
            let mut mutated = value.clone();
            for _ in 0..1 + rng.below(3) {
                mutate_value(&mut mutated, &mut rng);
            }
            let Ok(bytes) = encode_value(&mutated) else {
                continue;
            };
            assert_same_outcome(
                &bytes,
                &mutated,
                &format!("{name} mutation {round}"),
                &mut stats,
            );
        }
    }
    println!(
        "differential value mutations: {} ok, {} err",
        stats[0], stats[1]
    );
    assert!(stats[0] >= 10 && stats[1] >= 10, "{stats:?}");
}

#[test]
fn differential_binary_vs_json_on_corrupted_bytes() {
    let mut stats = [0u32; 2];
    let mut decode_errors = 0u32;
    let mut rng = Rng(0xb17e_f11d_0000_0002);
    for (name, value) in example_values() {
        let seed = encode_value(&value).unwrap();
        if seed.len() > 100_000 {
            continue;
        }
        for round in 0..150 {
            let mut bytes = seed.clone();
            for _ in 0..1 + rng.below(3) {
                let at = 32 + rng.below(bytes.len() - 32);
                match rng.below(3) {
                    0 => bytes[at] ^= 1 << rng.below(8),
                    1 => bytes[at] = rng.next() as u8,
                    _ => bytes[at] = bytes[at].wrapping_add(1),
                }
            }
            fix_crc(&mut bytes);
            let label = format!("{name} corruption {round}");
            match decode_to_value(&bytes) {
                Ok(decoded) => assert_same_outcome(&bytes, &decoded, &label, &mut stats),
                Err(expected) => {
                    assert_eq!(
                        Project::from_binary(&bytes).unwrap_err(),
                        expected,
                        "{label}"
                    );
                    decode_errors += 1;
                }
            }
        }
    }
    println!(
        "differential byte corruption: {} ok, {} err, {decode_errors} decode errors",
        stats[0], stats[1]
    );
    assert!(stats[1] + decode_errors >= 10);
}

#[test]
fn differential_edge_documents() {
    let mut base = serde_json::to_value(Project::default()).unwrap();
    base["nodes"] = json!([{"id":"a","name":"A","kind":{"type":"path","d":"M0 0 L10 0 L10 10 Z","bounds":[0.0,0.0,10.0,10.0]}}]);
    let mut stats = [0u32; 2];
    type Change = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, Change)> = vec![
        ("baseline", Box::new(|_| {})),
        ("int width", Box::new(|v| v["width"] = json!(640))),
        (
            "float schema",
            Box::new(|v| v["schemaVersion"] = json!(1.0)),
        ),
        (
            "negative schema",
            Box::new(|v| v["schemaVersion"] = json!(-1)),
        ),
        (
            "lossy width",
            Box::new(|v| v["width"] = json!(9007199254740993u64)),
        ),
        (
            "bounds as ints",
            Box::new(|v| v["nodes"][0]["kind"]["bounds"] = json!([0, 0, 10, 10])),
        ),
        (
            "bounds too long",
            Box::new(|v| v["nodes"][0]["kind"]["bounds"] = json!([0.0, 0.0, 1.0, 2.0, 3.0])),
        ),
        (
            "bounds too short",
            Box::new(|v| v["nodes"][0]["kind"]["bounds"] = json!([0.0, 1.0])),
        ),
        (
            "floats for string",
            Box::new(|v| v["name"] = json!([1.5, 2.5])),
        ),
        (
            "floats for width",
            Box::new(|v| v["width"] = json!([1.5, 2.5])),
        ),
        (
            "unknown in floats owner",
            Box::new(|v| v["nodes"][0]["kind"]["zz"] = json!(1)),
        ),
        (
            "variables map",
            Box::new(|v| {
                v["variables"] = json!({"b":1,"a":2.5,"c":"s","d":[1.5,2.5],"e":{"x":null}})
            }),
        ),
        (
            "variables float list",
            Box::new(|v| v["variables"] = json!({"a":[0.5,1.5,2.5]})),
        ),
        (
            "empty optionals",
            Box::new(|v| {
                v["followers"] = json!([]);
                v["groups"] = json!([]);
                v["guides"] = json!([]);
            }),
        ),
        (
            "legacy speech",
            Box::new(|v| v["speechCues"] = json!([{"x":1}])),
        ),
        (
            "dataBase64",
            Box::new(|v| {
                v["audioAssets"] =
                    json!([{"id":"s","name":"n","mimeType":"audio/wav","dataBase64":"AAAA"}])
            }),
        ),
        (
            "dataBase64 noncanonical",
            Box::new(|v| {
                v["audioAssets"] =
                    json!([{"id":"s","name":"n","mimeType":"audio/wav","dataBase64":"AAA"}])
            }),
        ),
        (
            "duplicate ids",
            Box::new(|v| {
                v["nodes"] = json!([
            {"id":"a","name":"A","kind":{"type":"circle","cx":0,"cy":0,"radius":1}},
            {"id":"a","name":"B","kind":{"type":"circle","cx":0,"cy":0,"radius":1}}])
            }),
        ),
        ("root array", Box::new(|v| *v = json!([1, 2, 3]))),
        ("root scalar", Box::new(|v| *v = json!(5))),
    ];
    for (label, change) in cases {
        let mut value = base.clone();
        change(&mut value);
        let bytes = encode_value(&value).unwrap();
        assert_same_outcome(&bytes, &value, label, &mut stats);
    }
    assert!(stats[0] >= 5 && stats[1] >= 5, "{stats:?}");
}

#[test]
fn strict_fallback_is_bounded_for_huge_invalid_documents() {
    // Small documents keep the exact reference error...
    let small = serde_json::json!({"schemaVersion": 1, "junk": [null, null]});
    let err = Project::from_bytes(&encode_value(&small).unwrap())
        .unwrap_err()
        .0;
    assert!(err.starts_with("Unsupported field at /junk"), "{err}");
    // ...huge ones fail without building a `Value` per arena node.
    let nulls = vec![Value::Null; splinely_document::FALLBACK_MAX_VALUES];
    let huge = serde_json::json!({"schemaVersion": 1, "junk": nulls});
    let err = Project::from_bytes(&encode_value(&huge).unwrap())
        .unwrap_err()
        .0;
    assert!(err.contains("slow path"), "{err}");
}
