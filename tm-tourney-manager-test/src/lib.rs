#[test]
fn it_works() {
    if cfg!(target_os = "windows") {
        let dir = env!("CARGO_MANIFEST_DIR").rsplit_once(r#"\"#).unwrap().0;

        let path = "file://".to_string() + dir + r#"\tm-tourney-manager "#;
        println!("{path}");

        let out = std::process::Command::new("cargo")
            .args([
                "build",
                "--package",
                &path,
                "--target",
                "wasm32-unknown-unknown",
            ])
            .output()
            .expect("failed to execute process");

        println!("{out:?}");
    }
}
