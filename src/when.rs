//! Tools to stub the implementation or return value of your mocks

mod once;

use crate::{matcher::{self, Any}, mock::{Mock, MockTimes, Stub}, mock_store::MockStore};
use once::Once;

pub use once::Once as WhenOnce;

/// Provides methods to stub the implementation or return value of the
/// mocked method.
///
/// Created using [`when!`].
///
/// By default, the stubbing will be setup for any argument. See
/// [`when!`] for an ergonomic way to set argument matchers. See
/// [`with_args`] for the full power or argument matching if the macro
/// is not enough.
///
/// By default, all methods are mocked indefinitely and the mock
/// closures may not consume captured variables. See the [`times`] and
/// [`once`] methods to override these default.
///
/// [`when!`]: crate::when!
/// [`once`]: When::once
/// [`times`]: When::times
/// [`with_args`]: When::with_args
pub struct When<'q, R, I, O, M: matcher::AllArgs<I>> {
    id: fn(R, I) -> O,
    store: &'q mut MockStore,
    times: MockTimes,
    matcher: M,
}

impl<'q, R, I, O> When<'q, R, I, O, Any> {
    #[doc(hidden)]
    pub fn new(id: fn(R, I) -> O, store: &'q mut MockStore) -> Self {
        When {
            id,
            store,
            matcher: Any,
            times: MockTimes::Always,
        }
    }
}

impl<'q, R, I, O: 'static, M: matcher::AllArgs<I> + Send + 'static> When<'q, R, I, O, M> {
    /// Sets the stub return value for the mocked method.
    ///
    /// Requires the value to be static. For a more lax but unsafe
    /// alternative, see: [`then_unchecked_return`]
    ///
    /// # Usage
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn multi_args(&mut self, a: &i32, b: i8) -> u32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   faux::when!(mock.multi_args).then_return(5);
    ///   assert_eq!(mock.multi_args(&2, 3), 5);
    ///   // mock activates multiple times
    ///   assert_eq!(mock.multi_args(&2, 3), 5);
    /// }
    /// ```
    ///
    /// [`then_unchecked_return`]: When::then_unchecked_return
    pub fn then_return(self, value: O)
    where
        O: Send + Clone,
    {
        self.then(move |_: I| value.clone());
    }

    /// Sets the stub implementation for the mocked method.
    ///
    /// The input for the closure is a tuple of all its non-receiver
    /// parameters
    ///
    /// The provided mock must be static and it must be mocking a
    /// method with static output. For a more lax but unsafe
    /// alternative, see: [`then_unchecked`].
    ///
    /// # Usage
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn no_args(&mut self) -> i32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    ///
    ///     pub fn single_arg(&self, a: u8) -> Vec<i8> {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    ///
    ///     pub fn multi_args(self, a: &i32, b: i8) -> u32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    ///
    ///     pub fn out_ref(&self) -> &u32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   // methods with no params
    ///   faux::when!(mock.no_args).then(|_| 5);
    ///   assert_eq!(mock.no_args(), 5);
    ///
    ///   // methods with a single param
    ///   faux::when!(mock.single_arg).then(|input| vec![input as i8]);
    ///   assert_eq!(mock.single_arg(8), vec![8]);
    ///
    ///   // methods with multiple params - some can be references
    ///   faux::when!(mock.multi_args).then(|(&a, _)| a as u32);
    ///   assert_eq!(mock.multi_args(&5, 2), 5);
    ///
    ///   // cannot mock methods that return references
    ///   // let x = 5;
    ///   // faux::when!(mock.out_ref).then(|_| &x);
    /// }
    /// ```
    ///
    /// [`then_unchecked`]: When::then_unchecked
    pub fn then(self, mock: impl FnMut(I) -> O + 'static + Send) {
        self.store.mock(
            self.id,
            Mock::new(
                Stub::Many {
                    times: self.times,
                    stub: Box::new(mock),
                },
                self.matcher,
            ),
        );
    }
}

impl<'q, R, I, O, M: matcher::AllArgs<I> + Send + 'static> When<'q, R, I, O, M> {
    /// Analog of [`then_return`] that allows stubbing non-static
    /// return values
    ///
    /// # Usage
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn multi_args(&mut self, a: &i32, b: i8) -> &u32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   let x = 5;
    ///   unsafe { faux::when!(mock.multi_args).then_unchecked_return(&x) }
    ///   assert_eq!(*mock.multi_args(&2, 3), x);
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// The lifetime of the returned object is not checked and can
    /// cause memory safety issues if used incorrectly.
    ///
    /// If the owner of the borrowed data is dropped while the
    /// captured reference is still accessible, a use-after-free
    /// violation will be triggered.
    ///
    /// This method can also cause aliasing issues where multiple
    /// mutable references are held for the same object.
    ///
    /// ### Example:
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn out_ref(&self) -> &u32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   let x = 5;
    ///   unsafe { faux::when!(mock.out_ref).then_unchecked_return(&x) }
    ///   std::mem::drop(x);
    ///   // assert_eq!(*mock.ref_return(), 5); // <~~ UB: use after free
    /// }
    /// ```
    ///
    /// [`then_return`]: When::then_return
    pub unsafe fn then_unchecked_return(self, value: O)
    where
        O: Send + Clone,
    {
        self.then_unchecked(move |_: I| value.clone())
    }

    /// Analog of [`then`] that allows stubbing implementations with
    /// non-static closures
    ///
    /// # Usage
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn out_ref(&mut self) -> &i32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   // the output can be a reference to the environment
    ///   // but this can be *very* dangerous so be careful
    ///   let x = 5;
    ///   unsafe { faux::when!(mock.out_ref).then_unchecked(|_| &x) }
    ///   assert_eq!(*mock.out_ref(), x);
    /// }
    ///
    /// ```
    ///
    ///
    /// # Safety
    ///
    /// The lifetimes of the outputs and captured variables are not
    /// checked. While this gives the caller maximum flexibility when
    /// mocking, it is *not* memory safe when used incorrectly.
    ///
    /// If the mocked method is called after its captured variables
    /// are dropped then a use-after-fress violation will be
    /// triggered.
    ///
    /// Relationships between inputs, outputs, and captured variable
    /// lifetimes are lost. This allows for easy violations of Rust's
    /// aliasing checks, creating undefined behavior.
    ///
    /// ### Example:
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn out_ref(&self, a : &mut i32) -> &mut i32 {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///   // the output is the same reference as the input
    ///   // the lifetimes of the input and output are thus linked
    ///   unsafe { faux::when!(mock.out_ref).then_unchecked(|i| i) }
    ///
    ///   let mut x = 5;
    ///   // y (the output) is a mutable reference back to x (the input)
    ///   // but there is no compile-time link between the two
    ///   let y = mock.out_ref(&mut x);
    ///
    ///   // We can check that they are both the same value
    ///   assert_eq!(*y, 5);
    ///   assert_eq!(x, 5);
    ///
    ///   // changes in x are reflected in y.
    ///   // This is UB and is not allowed in safe Rust!
    ///   x += 1;
    ///   assert_eq!(x, 6);
    ///   assert_eq!(*y, 6);
    ///
    ///   // and if we change y then x also gets changed
    ///   *y += 1;
    ///   assert_eq!(x, 7);
    ///   assert_eq!(*y, 7);
    /// }
    /// ```
    ///
    /// [`then`]: When::then
    pub unsafe fn then_unchecked(self, mock: impl FnMut(I) -> O + Send) {
        self.store.mock_unchecked(
            self.id,
            Mock::new(
                Stub::Many {
                    times: self.times,
                    stub: Box::new(mock),
                },
                self.matcher,
            ),
        );
    }

    /// Limits the number of times a mock is active.
    ///
    /// Calls past the limit result in a panic.
    ///
    /// # Usage
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn single_arg(&self, a: u8) -> Vec<i8> {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   // limit to 5 times
    ///   faux::when!(mock.single_arg)
    ///       .times(5)
    ///       .then(|input| vec![input as i8]);
    ///
    ///   // can be called 5 times safely
    ///   for _ in 0..5 {
    ///     assert_eq!(mock.single_arg(8), vec![8]);
    ///   }
    /// }
    /// ```
    ///
    /// ## Panics
    ///
    /// Panics if the mock is called more times than the specified
    /// number of times
    ///
    /// ```rust should_panic
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn single_arg(&self, a: u8) -> Vec<i8> {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   // limit to 5 times
    ///   faux::when!(mock.single_arg)
    ///       .times(5)
    ///       .then(|input| vec![input as i8]);
    ///
    ///   // panics on the 6th call
    ///   for _ in 0..6 {
    ///     assert_eq!(mock.single_arg(8), vec![8]);
    ///   }
    /// }
    /// ```
    pub fn times(mut self, times: usize) -> Self {
        self.times = MockTimes::Times(times);
        self
    }

    /// Limits mock to one call, allowing mocks to consume captured variables.
    ///
    /// Panics if the mock is called more than once.
    ///
    /// # Usage
    ///
    /// ```rust
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn single_arg(&self, a: u8) -> Vec<i8> {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   let vec = vec![25];
    ///   //moves vec to the closure
    ///   faux::when!(mock.single_arg).once().then(|_| vec);
    ///   assert_eq!(mock.single_arg(8), vec![25]);
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// ```rust should_panic
    /// #[faux::create]
    /// pub struct Foo {}
    ///
    /// #[faux::methods]
    /// impl Foo {
    ///     pub fn single_arg(&self, a: u8) -> Vec<i8> {
    ///       /* implementation code */
    ///       # panic!()
    ///     }
    /// }
    ///
    /// fn main() {
    ///   let mut mock = Foo::faux();
    ///
    ///   let vec = vec![25];
    ///   faux::when!(mock.single_arg).once().then(|_| vec);
    ///   assert_eq!(mock.single_arg(8), vec![25]);
    ///   //panics on its 2nd call
    ///   mock.single_arg(8);
    /// }
    /// ```
    pub fn once(self) -> Once<'q, R, I, O, M> {
        Once::new(self.id, self.store, self.matcher)
    }

    /// Specifies a matcher for all of the input arguments.
    ///
    /// This matcher must be satisfied for the stub to be invoked.
    ///
    /// See [`when!`](crate::when!) for a an ergonomic way to set the
    /// matchers
    ///
    /// If all the arguments implement [`Debug`](std::fmt::Debug) then
    /// a tuple of [`matcher::ArgMatcher`] can be provided where each
    /// `ArgMatcher` matches an individual argument of the method. If
    /// the method only has a single argument you can use
    /// [`matcher::Single`] to wrap the single `ArgMatcher`.
    ///
    /// For more complex use cases see [`matcher::AllArgs`].
    pub fn with_args<N: matcher::AllArgs<I> + Send + 'static>(
        self,
        matcher: N,
    ) -> When<'q, R, I, O, N> {
        When {
            matcher,
            times: self.times,
            id: self.id,
            store: self.store,
        }
    }
}
