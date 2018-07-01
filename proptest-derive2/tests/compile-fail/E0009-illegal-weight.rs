#[macro_use]
extern crate proptest_derive;

#[derive(Debug, Arbitrary)] //~ ERROR: 2 errors:
                            //~| [proptest_derive, E0009]
                            //~| [proptest_derive, E0030]
#[proptest(no_params)]
#[proptest(weight = 1)]
struct A {}

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
#[proptest(weight = 2)]
struct B;

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
#[proptest(weight = 3)]
struct C();

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
#[proptest(weight = 3)]
struct D { field: String }

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
#[proptest(weight = 3)]
struct E(Vec<u8>);

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
#[proptest(weight = 3)]
enum F { V1, V2, }

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
struct G(
    #[proptest(weight = 3)]
    Vec<u8>
);

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
struct H {
    #[proptest(weight = 3)]
    field: Vec<u8>
}

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
enum I {
    V0 {
        #[proptest(weight = 3)]
        field: Vec<u8>
    }
}

#[derive(Debug, Arbitrary)] //~ ERROR: [proptest_derive, E0009]
enum J {
    V0(#[proptest(weight = 3)] Vec<u8>)
}
