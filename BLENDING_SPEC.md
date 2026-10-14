# Weighted animation blending — additive schema 1

Frozen contract for core, runtime, CLI and editor integration.

Clip gains `blendMode` enum `override` (default) / `weighted`, `weight` default 1. Rust BlendMode { Override, Weighted }, Clip.blend_mode: BlendMode, Clip.weight:f64. EvaluationContext.clip_weights: BTreeMap<String,f64> default empty. Action::BlendClip { clip_id:String, weight:f64, duration:f64 } tagged `blendClip`, clipId camelCase, duration default0. Weight finite [0,1]; duration finite>=0; action requires existing weighted clip. Does not implicitly activate/restart/stop.

Preserve legacy priority ordering. Contiguous weighted clips are one group. Override clips flush group and retain old semantics. For each target/property independently: B is value before group, S=sum contributing clip weights, D=max(1,S). result = B*max(0,1-S) + sum(sample_i*weight_i/D). Zero contributions preserve baseline (including absent stroke). Positive stroke contributions use transparent baseline when absent. Numbers and RGBA weighted; rotation remains numeric scalar. Missing tracks not included in S. Local bones blended before FK. State overrides precede clips; explicit overrides last. Validate context weight IDs/ranges including inactive.

Runtime clip handle `weight` getter, `setWeight(weight)` and `fadeTo(weight,seconds)` return handle. Immutable project stays unchanged. Ramps linear from current synchronized weight; interrupted ramps start at current value; advance with segmented tick deltas before render/callback boundaries. Paused/ended clips may fade, production ticking continues during ramps. stop cancels ramp retains weight; seek changes sampletime not fade. Zero-duration immediate. Fade-zero does not stop. Visual weights do not affect audio volume. `blendClip` action starts ramp.

Two independent machines are parallel states, not themselves weighted blending. Runner demo: scrolling scene override tracks, adjacent weighted walk/jump bone tracks, click transitions fade walk->0/jump->1, jump completion fade back; sparse vertical jump channel returns baseline. Distinguish native preview and browser runtime implementation, do not silently drop action.

Tests legacy, midpoint/residual/normalized overweight, sparse, mixed boundaries, RGBA, bones FK, overrides last, invalid inputs, immutable JSON preservation, interrupted/paused fades, segmented parity and scroll during jump.
