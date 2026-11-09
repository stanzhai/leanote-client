fn main() {
    let svg = leanote_reader::math::render_math_svg(r"\partial\dot A/\partial A > 0", false, false).unwrap();
    std::fs::write("/tmp/formula.svg", svg).unwrap();
    let blk = leanote_reader::math::render_math_svg(r"\frac{\partial u}{\partial t} = \sum_{i=1}^n i", true, false).unwrap();
    std::fs::write("/tmp/formula_block.svg", blk).unwrap();
}
