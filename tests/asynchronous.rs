#![allow(clippy::disallowed_names)]

#[faux::create]
pub struct Foo {}

#[faux::methods]
impl Foo {
    pub async fn new() -> Self {
        Foo {}
    }

    pub async fn associated() -> u32 {
        5
    }

    pub async fn fetch(&self) -> i32 {
        self.private().await
    }

    async fn private(&self) -> i32 {
        3
    }
}

#[test]
fn real_instance() {
    let fetched = futures::executor::block_on(async {
        let foo = Foo::new().await;
        foo.fetch().await
    });

    assert_eq!(fetched, 3);
}

#[test]
fn mocked() {
    let mut foo = Foo::faux();
    faux::when!(foo.fetch).then(|_| 10);
    let fetched = futures::executor::block_on(foo.fetch());
    assert_eq!(fetched, 10);
}

#[test]
fn unmocked_faux_should_track_caller_location() {
    use std::panic;
    use std::sync::{Arc, Mutex};

    let mock = Foo::faux();

    let panic_location = Arc::new(Mutex::new(None));
    let cloned = Arc::clone(&panic_location);

    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let mut panic_location = cloned.lock().unwrap();
        let location = info.location().unwrap();
        *panic_location = Some(location.to_string());
    }));

    let _ = panic::catch_unwind(|| futures::executor::block_on(mock.fetch()));

    panic::set_hook(prev_hook);

    assert_eq!(
        "tests/asynchronous.rs:60:69",
        panic_location.lock().unwrap().take().unwrap()
    );
}
