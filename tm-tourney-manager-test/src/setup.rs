use tm_tourney_manager_api_rs::DbConnection;

pub(crate) fn test() -> DatabaseRunner {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::SyncRunner,
    };

    let dir = env!("CARGO_MANIFEST_DIR").rsplit_once(r#"\"#).unwrap().0;

    let path = "file://".to_string() + dir + r#"\"# + super::PROJECT_NAME;

    std::process::Command::new("cargo")
        .args([
            "build",
            "--package",
            &path,
            "--target",
            "wasm32-unknown-unknown",
        ])
        .output()
        .expect("failed to build your spacetime server module.");

    let _spacetime_container = SyncRunner::start(
        GenericImage::new("clockworklabs/spacetime", "v1.9.0")
            .with_exposed_port(3000.tcp())
            .with_wait_for(WaitFor::message_on_stdout(
                "spacetimedb-standalone version: 1.9.0",
            ))
            .with_cmd(["start"]),
    )
    .unwrap();

    let spacetime_url = format!(
        "http://{}:{}",
        _spacetime_container.get_host().unwrap(),
        _spacetime_container.get_host_port_ipv4(3000).unwrap()
    );

    let published_module_name: String = "testingthething".into();

    let published_dir = dir.to_string()
        + "/target/wasm32-unknown-unknown/debug/"
        + &super::PROJECT_NAME.replace("-", "_")
        + ".wasm";

    std::process::Command::new("spacetime")
        .args([
            "publish",
            "-b",
            &published_dir,
            "-s",
            &spacetime_url,
            &published_module_name,
        ])
        .output()
        .expect("failed to build your spacetime server module.");

    let spacetime = DbConnection::builder()
        .with_module_name(published_module_name)
        .with_uri(spacetime_url)
        .build()
        .expect("Failed to connect to SpacetimeDB. Aborting.");

    DatabaseRunner {
        _spacetime_container,
        spacetime,
        messages: std::cell::RefCell::new(0),
    }
}

pub(crate) struct DatabaseRunner {
    _spacetime_container: testcontainers::Container<testcontainers::GenericImage>,
    spacetime: DbConnection,
    messages: std::cell::RefCell<u32>,
}

impl DatabaseRunner {
    pub(crate) fn wait_for_msgs(&self) {
        let mut msg = 0;
        while msg < *self.messages.borrow() {
            self.spacetime.advance_one_message_blocking().unwrap();
            msg += 1;
        }
    }
}

/* impl Drop for DatabaseRunner {
    fn drop(&mut self) {
        let mut msg = 0;
        while msg < *self.messages.borrow() {
            self.spacetime.advance_one_message().unwrap();
            msg += 1;
        }
    }
} */

impl std::ops::Deref for DatabaseRunner {
    type Target = DbConnection;

    fn deref(&self) -> &Self::Target {
        // The 5 is an arbitrary value which sounds good to my ears :shrug:.
        *self.messages.borrow_mut() += 5;
        &self.spacetime
    }
}
