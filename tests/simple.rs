#[faux::create]
pub struct Foo {
    a: u32,
}

#[faux::methods]
impl Foo {
    pub fn new(a: u32) -> Self {
        Foo { a }
    }

    pub fn get_stuff(&self) -> u32 {
        self.a
    }

    pub fn add_stuff(&self, x: i32) -> i32 {
        self.a as i32 + x
    }

    pub fn add_stuff_2(&self, x: i32, y: &i32) -> i32 {
        self.a as i32 + x + y
    }

    pub fn ret_ref(&self, _: &u32) -> &u32 {
        &self.a
    }
}

fn load_a() -> Result<u32, Box<dyn std::error::Error>> {
    Ok(3)
}

// tests that functions not tagged by `faux::methods` can use the ones
// that are in a `faux::methods` impl block
impl Foo {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let a = load_a()?;
        Ok(Foo::new(a))
    }
}

#[test]
fn real_struct() {
    let real = Foo::new(3);
    assert_eq!(real.get_stuff(), 3);
    assert_eq!(real.add_stuff(2), 5);
}

#[test]
fn faux_single_arg() {
    let mut mock = Foo::faux();
    faux::when!(mock.get_stuff).then(|_| 10);
    assert_eq!(mock.get_stuff(), 10);
}

#[test]
fn faux_multi_arg() {
    let mut mock = Foo::faux();
    faux::when!(mock.add_stuff_2).then(|(a, &b)| a - b);
    assert_eq!(mock.add_stuff_2(90, &30), 60);
}

#[test]
fn faux_ref_output() {
    let mut mock = Foo::faux();
    unsafe { faux::when!(mock.ret_ref).then_unchecked(|a| a) };
    let x = 30 + 30;
    assert_eq!(*mock.ret_ref(&x), 60);
}

#[test]
#[should_panic(expected = "`Foo::get_stuff` was called but never stubbed")]
fn unmocked_faux_panics() {
    let mock = Foo::faux();
    mock.get_stuff();
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

    let _ = panic::catch_unwind(|| mock.get_stuff());

    panic::set_hook(prev_hook);

    assert_eq!(
        "tests/simple.rs:95:41",
        panic_location.lock().unwrap().take().unwrap()
    );
}
