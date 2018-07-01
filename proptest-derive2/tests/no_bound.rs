// Copyright 2018 Mazdak Farrokhzad
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[macro_use]
extern crate proptest_derive;
extern crate proptest;
use proptest::prelude::Arbitrary;

#[derive(Debug)]
struct NotArbitrary;

/// Ensure that we can't determine that this is PhantomData syntactically.
type HidePH<T> = ::std::marker::PhantomData<T>;

#[derive(Debug, Arbitrary)]
struct T1<#[proptest(no_bound)] T>(HidePH<T>);

#[derive(Debug, Arbitrary)]
struct T2(T1<NotArbitrary>);

#[derive(Debug, Arbitrary)]
struct T3<
    #[proptest(no_bound)] A,
    B,
    #[proptest(no_bound)] G,
> {
    alpha: HidePH<A>,
    beta: B,
    gamma: HidePH<G>,
}

#[derive(Debug, Arbitrary)]
struct T4(T3<NotArbitrary, bool, NotArbitrary>);

/*
TODO:

#[derive(Debug, Arbitrary)]
#[proptest(no_bound)]
struct Foo<A, B, C>(HidePH<(A, B, C)>);

*/

#[test]
fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<T2>();
    assert_arbitrary::<T4>();
}
