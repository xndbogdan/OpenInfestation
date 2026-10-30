use super::*;

#[path = "quad_effect_tests.rs"]
mod integration;

#[test]
fn first_triangle_cull_uses_signed_words_and_wrapping_products() {
    let p = super::super::effect::pack_xy;
    assert!(positive_area([p(0, 0), p(100, 0), p(0, 100)]));
    assert!(!positive_area([p(0, 0), p(0, 100), p(100, 0)]));
    assert!(!positive_area([p(0, 0); 3]));
    assert!(!positive_area([
        p(-32768, -32768),
        p(32767, -32768),
        p(-32768, 32767)
    ]));
}
