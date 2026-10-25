//! `ig_core::review` plus printing the result.

pub use ig_core::review::*;

pub fn print(head: &[ReviewItem], rest: usize, all: bool, rest_items: &[ReviewItem]) {
    let show = |i: usize, it: &ReviewItem| {
        println!("{}. {} [{}]", i + 1, it.title, it.rule_id);
        println!("   {}", it.evidence);
        println!("   why: {}", it.why);
        println!("   fix: {}", it.fix);
    };
    if head.is_empty() {
        println!("ig review: no structural problems found");
        return;
    }
    for (i, it) in head.iter().enumerate() {
        show(i, it);
    }
    if all {
        for (i, it) in rest_items.iter().enumerate() {
            show(head.len() + i, it);
        }
    } else if rest > 0 {
        println!("\n{rest} more — ig review --all");
    }
}
