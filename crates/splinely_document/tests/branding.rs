use splinely_document::Project;

#[test]
fn static_and_interactive_exports_identify_splinely_without_watermark() {
    let project = Project::default();
    for svg in [
        project.to_static_svg(0.0).unwrap(),
        project.to_svg("").unwrap(),
    ] {
        let doc = roxmltree::Document::parse(&svg).unwrap();
        assert!(svg.contains("Created with Splinely "));
        let generator = doc
            .descendants()
            .find(|node| node.has_tag_name(("urn:splinely:metadata", "generator")))
            .unwrap();
        assert_eq!(generator.attribute("name"), Some("Splinely"));
        assert_eq!(
            generator.attribute("version"),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(generator.attribute("schemaVersion"), Some("1"));
        assert!(generator
            .ancestors()
            .any(|n| n.has_tag_name(("http://www.w3.org/2000/svg", "metadata"))));
    }
}
