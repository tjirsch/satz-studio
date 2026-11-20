//! The path through an estate, as the rail orders it: what the estate is, its presets and packs,
//! their questions, its way to the organization, its documents. While a step's view is
//! showing, the window's foot carries a bar that goes back to the step before it and on to
//! the next one, outside the view's scrolling content so it never scrolls away
//! ([ADR 0028](../../../../docs/adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)).
//! The Estate destination is not a step: it is the estate file itself, for whoever wants
//! a value the questions do not ask for.

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View};

/// The steps, in order. An estate without a Presets destination skips that one.
const STEPS: [View; 6] = [
    View::Overview,
    View::Presets,
    View::Packs,
    View::Decisions,
    View::Apply,
    View::Documents,
];

/// The step after `from`, or `None` for the last step and for a view that is not one.
pub fn next_after(from: View, presets: bool) -> Option<View> {
    let at = STEPS.iter().position(|v| *v == from)?;
    STEPS[at + 1..]
        .iter()
        .copied()
        .find(|v| presets || *v != View::Presets)
}

/// The step before `from`, or `None` for the first step and for a view that is not one.
/// The inverse of [`next_after`]: an estate without presets skips them going back too.
pub fn previous_before(from: View, presets: bool) -> Option<View> {
    let at = STEPS.iter().position(|v| *v == from)?;
    STEPS[..at]
        .iter()
        .rev()
        .copied()
        .find(|v| presets || *v != View::Presets)
}

/// What the next step is for, in one sentence.
pub fn purpose(step: View) -> &'static str {
    match step {
        View::Presets => "Choose the presets this organization is built from.",
        View::Packs => "Take the packs this estate is built from.",
        View::Decisions => "Answer what the estate's presets and packs ask.",
        View::Apply => "Check who is signed in, then check, plan and apply.",
        View::Documents => "Make the documents the customer receives.",
        _ => "",
    }
}

/// The bar at the window's foot while a step is showing, left to right: "Back: <step>" to
/// the step before, "Next: <step>", and what the next step is for. The two buttons stand
/// together on the left and never shrink; where one is missing the other keeps the left
/// edge. The description gives way — one line, cut with an ellipsis, down to nothing. Nothing on a view that
/// is not a step, and nothing without an open estate.
#[component]
pub fn PathBar() -> Element {
    let app = use_context::<Store<AppStore>>();
    let at = app.nav().cloned();
    if app.open().is_none() || !STEPS.contains(&at) {
        return rsx! {};
    }
    let presets = app
        .estate()
        .model()
        .read()
        .as_deref()
        .is_none_or(|m| m.packs.has_presets());
    let previous = previous_before(at, presets);
    let next = next_after(at, presets);
    rsx! {
        div { class: "path-bar", role: "toolbar", "aria-label": "Path",
            if let Some(previous) = previous {
                Button {
                    variant: ButtonVariant::Text,
                    icon: "arrow_back",
                    class: "path-bar__back",
                    onclick: move |_| app.nav().set(previous),
                    "Back: {previous.label()}"
                }
            }
            if let Some(next) = next {
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "arrow_forward",
                    class: "path-bar__next",
                    onclick: move |_| app.nav().set(next),
                    "Next: {next.label()}"
                }
                span { class: "path-bar__text", "{purpose(next)}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk is the rail's order without Estate; an estate without presets goes from
    /// the Overview to its packs.
    #[test]
    fn the_steps_follow_each_other_and_an_estate_without_presets_skips_them() {
        let walk = |presets: bool| {
            let mut at = View::Overview;
            let mut out = vec![at];
            while let Some(next) = next_after(at, presets) {
                out.push(next);
                at = next;
            }
            out
        };
        assert_eq!(walk(true), STEPS);
        assert_eq!(
            walk(false),
            [
                View::Overview,
                View::Packs,
                View::Decisions,
                View::Apply,
                View::Documents
            ]
        );
        // not a step, and the last step
        assert_eq!(next_after(View::Estate, true), None);
        assert_eq!(next_after(View::Documents, true), None);
    }

    /// Going back walks the same steps the other way; an estate without presets goes from
    /// its packs back to the Overview.
    #[test]
    fn the_steps_go_back_the_way_they_came_and_an_estate_without_presets_skips_them() {
        let walk = |presets: bool| {
            let mut at = View::Documents;
            let mut out = vec![at];
            while let Some(previous) = previous_before(at, presets) {
                out.push(previous);
                at = previous;
            }
            out.reverse();
            out
        };
        assert_eq!(walk(true), STEPS);
        assert_eq!(
            walk(false),
            [
                View::Overview,
                View::Packs,
                View::Decisions,
                View::Apply,
                View::Documents
            ]
        );
        // not a step, and the first step
        assert_eq!(previous_before(View::Estate, true), None);
        assert_eq!(previous_before(View::Overview, true), None);
        // each step on the walk is the next of the step before it
        for presets in [true, false] {
            for step in STEPS.into_iter().filter(|v| presets || *v != View::Presets) {
                if let Some(previous) = previous_before(step, presets) {
                    assert_eq!(next_after(previous, presets), Some(step), "{step:?}");
                }
            }
        }
    }

    /// Every step that can be gone to says what it is for, and is a primary destination.
    #[test]
    fn every_next_step_has_a_purpose_and_is_in_the_rail() {
        for step in &STEPS[1..] {
            assert!(!purpose(*step).is_empty(), "{step:?}");
            assert!(View::PRIMARY.contains(step), "{step:?}");
        }
    }
}
