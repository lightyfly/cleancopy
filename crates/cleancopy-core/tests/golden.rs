use std::{fs, path::PathBuf};

use cleancopy_core::{render_markdown, render_plain};

#[test]
fn fixture_snapshots() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut fixtures: Vec<_> = fs::read_dir(root.join("fixtures"))
        .expect("read workspace fixtures")
        .map(|entry| entry.expect("read fixture entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    fixtures.sort();
    assert!(!fixtures.is_empty(), "expected Markdown fixtures");

    insta::with_settings!({
        snapshot_path => root.join("tests/snapshots"),
        prepend_module_to_snapshot => false,
    }, {
        for fixture in fixtures {
            let name = fixture
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("UTF-8 fixture name");
            let input = fs::read_to_string(&fixture).expect("read UTF-8 fixture");
            // Both renderers use native line endings. Compare LF on every OS.
            let plain = render_plain(&input).replace("\r\n", "\n");
            let markdown = render_markdown(&input).replace("\r\n", "\n");
            // An EOF marker keeps trailing newlines visible in insta's text snapshots.
            insta::assert_snapshot!(format!("{name}__plain"), format!("{plain}<EOF>"));
            insta::assert_snapshot!(format!("{name}__markdown"), markdown);
        }
    });
}
