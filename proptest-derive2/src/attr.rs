// Copyright 2018 Mazdak Farrokhzad
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Provides a parser from syn attributes to our logical model.

use syn::{self, Meta, NestedMeta, Lit, Ident, Attribute, Expr, Type};

use util;
use interp;
use error;
use error::{Ctx, DeriveResult};

//==============================================================================
// Public API
//==============================================================================

/// Parsed attributes in our logical model.
#[derive(Clone)]
pub struct ParsedAttributes {
    /// If we've been ordered to skip this item.
    /// This is only valid for enum variants.
    pub skip: bool,
    /// The potential weight assigned to an enum variant.
    /// This must be `None` for things that are not enum variants.
    pub weight: Option<u32>,
    /// The mode for `Parameters` to use. See that type for more.
    pub params: ParamsMode,
    /// The mode for `Strategy` to use. See that type for more.
    pub strategy: StratMode,
    /// True if no_bound was specified.
    pub no_bound: bool,
}

/// The mode for the associated item `Strategy` to use.
#[derive(Clone)]
pub enum StratMode {
    /// This means that no explicit strategy was specified
    /// and that we thus should use `Arbitrary` for whatever
    /// it is that needs a strategy.
    Arbitrary,
    /// This means that an explicit value has been provided.
    /// The result of this is to use a strategy that always
    /// returns the given value.
    Value(Expr),
    /// This means that an explicit strategy has been provided.
    /// This strategy will be used to generate whatever it
    /// is that the attribute was set on.
    Strategy(Expr),
}

/// The mode for the associated item `Parameters` to use.
#[derive(Clone)]
pub enum ParamsMode {
    /// Nothing has been specified. The children are now free to
    /// specify their parameters, and if nothing is specified, then
    /// `<X as Arbitrary>::Parameters` will be used for a type `X`.
    Passthrough,
    /// We've been ordered to use the Default value of
    /// `<X as Arbitrary>::Parameters` for some field where applicable.
    /// For the top level item, this means that `Parameters` will be
    /// the unit type. For children, it means that this child should
    /// not count towards the product type that is being built up.
    Default,
    /// An explicit type has been specified on some item.
    /// If the top level item has this specified on it, this means
    /// that `Parameters` will have the given type.
    /// If it is specified on a child of the top level item, this
    /// entails that the given type will be added to the resultant
    /// product type.
    Specified(Type),
}

impl ParamsMode {
    /// Returns `true` iff the mode was explicitly set.
    pub fn is_set(&self) -> bool {
        if let ParamsMode::Passthrough = *self { false } else { true }
    }

    /// Converts the mode to an `Option` of an `Option` of a type
    /// where the outer `Option` is `None` iff the mode wasn't set
    /// and the inner `Option` is `None` iff the mode was `Default`.
    pub fn to_option(self) -> Option<Option<Type>> {
        use self::ParamsMode::*;
        match self {
            Passthrough => None,
            Specified(ty) => Some(Some(ty)),
            Default => Some(None),
        }
    }
}

impl StratMode {
    /// Returns `true` iff the mode was explicitly set.
    pub fn is_set(&self) -> bool {
        if let StratMode::Arbitrary = self { false } else { true }
    }
}

/// Parse the attributes specified on an item and parsed by syn
/// into our logical model that we work with.
pub fn parse_attributes(ctx: Ctx, attrs: Vec<Attribute>)
    -> DeriveResult<ParsedAttributes>
{
    let attrs = parse_attributes_base(ctx, attrs)?;
    if attrs.no_bound {
        error::no_bound_set_on_non_tyvar(ctx);
    }
    Ok(attrs)
}

/// Parses the attributes specified on an item and parsed by syn
/// and returns true if we've been ordered to not set an `Arbitrary`
/// bound on the given type variable the attributes are from,
/// no matter what.
pub fn has_no_bound(ctx: Ctx, attrs: Vec<Attribute>) -> DeriveResult<bool> {
    let attrs = parse_attributes_base(ctx, attrs)?;
    error::if_anything_specified(ctx, &attrs, error::TY_VAR)?;
    Ok(attrs.no_bound)
}

/// Parse the attributes specified on an item and parsed by syn
/// into our logical model that we work with.
fn parse_attributes_base(ctx: Ctx, attrs: Vec<Attribute>)
    -> DeriveResult<ParsedAttributes>
{
    let acc = parse_accumulate(ctx, attrs);

    Ok(ParsedAttributes {
        skip: acc.skip.is_some(),
        weight: acc.weight,
        // Process params and no_params together to see which one to use.
        params: parse_params_mode(ctx, acc.no_params, acc.params)?,
        // Process strategy and value together to see which one to use.
        strategy: parse_strat_mode(ctx, acc.strategy, acc.value)?,
        no_bound: acc.no_bound.is_some()
    })
}

//==============================================================================
// Internals: Initialization
//==============================================================================

#[derive(Default)]
struct ParseAcc {
    skip: Option<()>,
    weight: Option<u32>,
    no_params: Option<()>,
    params: Option<Type>,
    strategy: Option<Expr>,
    value: Option<Expr>,
    no_bound: Option<()>,
}

//==============================================================================
// Internals: Extraction & Filtering
//==============================================================================

fn parse_accumulate(ctx: Ctx, attrs: Vec<Attribute>) -> ParseAcc {
    let mut state = ParseAcc::default();

    // Get rid of attributes we don't care about:
    for attr in attrs.into_iter().filter(is_proptest_attr) {
        // Flatten attributes so we deal with them uniformly.
        state = extract_modifiers(ctx, attr).into_iter()
            // Accumulate attributes into a form for final processing.
            .fold(state, |state, meta| dispatch_attribute(ctx, state, meta))
    }

    state
}

/// Returns `true` iff the attribute has to do with proptest.
/// Otherwise, the attribute is irrevant to us and we will simply
/// ignore it in our processing.
fn is_proptest_attr(attr: &Attribute) -> bool {
    util::eq_simple_path("proptest", &attr.path)
}

/// Extract all individual attributes inside one `#[proptest(..)]`.
/// We do this to treat all pieces uniformly whether a single
/// `#[proptest(..)]` was used or many. This simplifies the
/// logic somewhat.
fn extract_modifiers(ctx: Ctx, attr: Attribute) -> Vec<Meta> {
    // Ensure we've been given an outer attribute form.
    if !is_outer_attr(&attr) {
        error::inner_attr(ctx);
    }

    match attr.interpret_meta() {
        Some(Meta::List(list)) => return list.nested.into_iter()
            .filter_map(|nm| extract_metas(ctx, nm)).collect(),
        Some(Meta::Word(_)) => error::bare_proptest_attr(ctx),
        Some(Meta::NameValue(_)) => error::literal_set_proptest(ctx),
        None => error::no_interp_meta(ctx),
    }

    vec![]
}

fn extract_metas(ctx: Ctx, nested: NestedMeta) -> Option<Meta> {
    match nested {
        NestedMeta::Literal(_) => {
            error::immediate_literals(ctx);
            None
        },
        // This is the only valid form.
        NestedMeta::Meta(meta) => Some(meta),
    }
}

/// Returns true iff the given attribute is an outer one, i.e: `#[<attr>]`.
/// An inner attribute is the other possibility and has the syntax `#![<attr>]`.
/// Note that `<attr>` is a meta-variable for the contents inside.
fn is_outer_attr(attr: &Attribute) -> bool {
    syn::AttrStyle::Outer == attr.style
}

//==============================================================================
// Internals: Dispatch
//==============================================================================

/// Dispatches an attribute modifier to handlers and
/// let's them add stuff into our accumulartor.
fn dispatch_attribute(ctx: Ctx, mut acc: ParseAcc, meta: Meta) -> ParseAcc {
    // TODO: revisit when we have NLL.

    // Dispatch table for attributes:
    //
    // N.B: We use this trick to return function pointers to avoid cloning.
    // Once we have NLL this might not be necessary.
    let parser = {
        let name = meta.name().to_string();
        let name = name.as_ref();
        match name {
            // Valid modifiers:
            "skip" => parse_skip,
            "w" | "weight" => parse_weight,
            "no_params" => parse_no_params,
            "params" => parse_params,
            "strategy" => parse_strategy,
            "value" => parse_value,
            "no_bound" => parse_no_bound,
            // Invalid modifiers:
            name => {
                dispatch_unknown_mod(ctx, name);
                return acc;
            },
        }
    };

    // We now have a parser that we can dispatch to.
    parser(ctx, &mut acc, meta);
    acc
}

fn dispatch_unknown_mod(ctx: Ctx, name: &str) {
    match name {
        "no_bounds" =>
            error::did_you_mean(ctx, name, "no_bound"),
        "weights" | "weighted" =>
            error::did_you_mean(ctx, name, "weight"),
        "strat" | "strategies" =>
            error::did_you_mean(ctx, name, "strategy"),
        "values" | "valued" | "fix" | "fixed" =>
            error::did_you_mean(ctx, name, "value"),
        "param" | "parameters" =>
            error::did_you_mean(ctx, name, "params"),
        "no_param" | "no_parameters" =>
            error::did_you_mean(ctx, name, "no_params"),
        name =>
            error::unkown_modifier(ctx, name),
    }
}

//==============================================================================
// Internals: no_bound
//==============================================================================

/// Parse a no_bound attribute.
/// Valid forms are:
/// + `#[proptest(no_bound)]`
fn parse_no_bound(ctx: Ctx, acc: &mut ParseAcc, meta: Meta) {
    parse_bare_modifier(ctx, &mut acc.no_bound, meta, error::no_bound_malformed)
}

//==============================================================================
// Internals: Skip
//==============================================================================

/// Parse a skip attribute.
/// Valid forms are:
/// + `#[proptest(skip)]`
fn parse_skip(ctx: Ctx, acc: &mut ParseAcc, meta: Meta) {
    parse_bare_modifier(ctx, &mut acc.skip, meta, error::skip_malformed)
}

//==============================================================================
// Internals: Weight
//==============================================================================

/// Parses a weight.
/// Valid forms are:
/// + `#[proptest(weight = <integer>)]`
/// + `#[proptest(weight = "<expr>")]`
/// + `#[proptest(weight(<integer>))]`
/// + `#[proptest(weight("<expr>""))]`
///
/// The `<integer>` must also fit within an `u32` and be unsigned.
fn parse_weight(ctx: Ctx, acc: &mut ParseAcc, meta: Meta) {
    use std::u32;
    error_if_set(ctx, &acc.weight, &meta);

    // Convert to value if possible:
    let value = extract_lit_expr(meta.clone())
        // Evaluate the expression into a value:
        .as_ref().and_then(interp::eval_expr)
        // Ensure that `val` fits within an `u32` as proptest requires that:
        .filter(|&value| value <= u128::from(u32::MAX))
        .map(|value| value as u32);

    if let Some(value) = value {
        acc.weight = Some(value);
    } else {
        error::weight_malformed(ctx, &meta)
    }
}

//==============================================================================
// Internals: Strategy
//==============================================================================

/// Parses an explicit value as a strategy.
/// Valid forms are:
/// + `#[proptest(value = <literal>)]`
/// + `#[proptest(value = "<expr>")]`
/// + `#[proptest(value("<expr>")]`
/// + `#[proptest(value(<literal>)]`
fn parse_value(ctx: Ctx, acc: &mut ParseAcc, meta: Meta) {
    parse_strategy_base(ctx, &mut acc.value, meta)
}

/// Parses an explicit strategy.
/// Valid forms are:
/// + `#[proptest(strategy = <literal>)]`
/// + `#[proptest(strategy = "<expr>")]`
/// + `#[proptest(strategy("<expr>")]`
/// + `#[proptest(strategy(<literal>)]`
fn parse_strategy(ctx: Ctx, acc: &mut ParseAcc, meta: Meta) {
    parse_strategy_base(ctx, &mut acc.strategy, meta)
}

/// Parses an explicit strategy. This is a helper.
/// Valid forms are:
/// + `#[proptest(<meta.name()> = <literal>)]`
/// + `#[proptest(<meta.name()> = "<expr>")]`
/// + `#[proptest(<meta.name()>("<expr>")]`
/// + `#[proptest(<meta.name()>(<literal>)]`
fn parse_strategy_base(ctx: Ctx, loc: &mut Option<Expr>, meta: Meta) {
    error_if_set(ctx, &loc, &meta);
    if let Some(expr) = extract_lit_expr(meta.clone()) {
        *loc = Some(expr);
    } else {
        error::strategy_malformed(ctx, &meta)
    }
}

/// Combines any parsed explicit strategy and value into a single value
/// and fails if both an explicit strategy and value was set.
/// Only one of them can be set, or none.
fn parse_strat_mode(ctx: Ctx, strat: Option<Expr>, value: Option<Expr>)
    -> DeriveResult<StratMode> 
{
    Ok(match (strat, value) {
        (None,     None    ) => StratMode::Arbitrary,
        (None,     Some(ty)) => StratMode::Value(ty),
        (Some(ty), None    ) => StratMode::Strategy(ty),
        (Some(_), Some(_) )  => error::overspecified_strat(ctx)?,
    })
}

//==============================================================================
// Internals: Parameters
//==============================================================================

/// Combines a potentially set `params` and `no_params` into a single value
/// and fails if both have been set. Only one of them can be set, or none.
fn parse_params_mode(ctx: Ctx, no_params: Option<()>, ty_params: Option<Type>)
    -> DeriveResult<ParamsMode>
{
    Ok(match (no_params, ty_params) {
        (None,    None    ) => ParamsMode::Passthrough,
        (None,    Some(ty)) => ParamsMode::Specified(ty),
        (Some(_), None    ) => ParamsMode::Default,
        (Some(_), Some(_) ) => error::overspecified_param(ctx)?,
    })
}

/// Parses an explicit Parameters type.
///
/// Valid forms are:
/// + `#[proptest(params(<type>)]`
/// + `#[proptest(params("<type>")]`
/// + `#[proptest(params = "<type>"]`
///
/// The latter form is required for more complex types.
fn parse_params(ctx: Ctx, acc: &mut ParseAcc, meta: Meta) {
    error_if_set(ctx, &acc.params, &meta);

    let typ = match normalize_meta(meta) {
        // Form is: `#[proptest(params(<type>)]`.
        Some(NormMeta::Word(ident)) => Some(ident_to_type(ident)),
        // Form is: `#[proptest(params = "<type>"]` or,
        // Form is: `#[proptest(params("<type>")]`..
        Some(NormMeta::Lit(Lit::Str(lit))) => lit.parse().ok(),
        _ => None,
    };

    if let Some(typ) = typ {
        acc.params = Some(typ);
    } else {
        error::param_malformed(ctx)
    }
}

/// Parses an order to use the default Parameters type and value.
/// Valid forms are:
/// + `#[proptest(no_params)]`
fn parse_no_params(ctx: Ctx, acc: &mut ParseAcc, meta: Meta) {
    parse_bare_modifier(ctx, &mut acc.no_params, meta, error::no_params_malformed)
}

//==============================================================================
// Internals: Utilities
//==============================================================================

/// Parses a bare attribute of the form `#[proptest(<attr>)]` and sets `loc`.
fn parse_bare_modifier
    (ctx: Ctx, loc: &mut Option<()>, meta: Meta, malformed: fn(Ctx))
{
    error_if_set(ctx, loc, &meta);

    if let Some(NormMeta::Plain) = normalize_meta(meta) {
        *loc = Some(());
    } else {
        malformed(ctx);
    }
}

/// Emits a "set again" error iff the given option `.is_some()`.
fn error_if_set<T>(ctx: Ctx, loc: &Option<T>, meta: &Meta) {
    if loc.is_some() {
        error::set_again(ctx, meta)
    }
}

/// Constructs a type out of an identifier.
fn ident_to_type(ident: Ident) -> Type {
    Type::Path(syn::TypePath { qself: None, path: ident.into() })
}

/// Extract a `lit` in `NormMeta::Lit(<lit>)`.
fn extract_lit(meta: Meta) -> Option<Lit> {
    if let NormMeta::Lit(lit) = normalize_meta(meta)? {
        Some(lit)
    } else {
        None
    }
}

/// Extract expression out of meta if possible.
fn extract_lit_expr(meta: Meta) -> Option<Expr> {
    match extract_lit(meta) {
        Some(Lit::Str(lit)) => lit.parse().ok(),
        Some(Lit::Int(lit)) => Some(
            Expr::from(syn::ExprLit { attrs: vec![], lit: lit.into() })
        ),
        _ => None,
    }
}

/// Normalized `Meta` into all the forms we will possibly accept.
#[derive(Debug)]
enum NormMeta {
    /// Accepts: `#[proptest(<word>)]`
    Plain,
    /// Accepts: `#[proptest(<word> = <lit>)]` and `#[proptest(<word>(<lit>))]`
    Lit(Lit),
    /// Accepts: `#[proptest(<word>(<word>))`.
    Word(Ident)
}

/// Normalize a `meta: Meta` into the forms accepted in `#[proptest(<meta>)]`.
fn normalize_meta(meta: Meta) -> Option<NormMeta> {
    Some(match meta {
        Meta::Word(_) => NormMeta::Plain,
        Meta::NameValue(nv) => NormMeta::Lit(nv.lit),
        Meta::List(ml) => if let Some(nm) = util::match_singleton(ml.nested) {
            match nm {
                NestedMeta::Literal(lit) => NormMeta::Lit(lit),
                NestedMeta::Meta(Meta::Word(word)) => NormMeta::Word(word),
                _ => return None
            }
        } else {
            return None
        },
    })
}
