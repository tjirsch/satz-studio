//! The path through an estate, as the sidebar orders it: what the estate is, its presets and packs,
//! their questions, the estate file for a value no question asks for, its way to the
//! organization, the security checks its resources fail, its documents. While a step's
//! view is showing, the window's foot carries a bar that goes back to the step before it
//! and on to the next one, outside the view's scrolling content so it never scrolls away
//! ([ADR 0028](../../../../docs/adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md),
//! [ADR 0037](../../../../docs/adr/0037-estate-is-a-step-of-the-path-before-apply.md)).
//! Every primary destination is a step; Agent and Settings are none.

use dioxus::prelude::*;

use dioxus_icons::lucide::{ArrowLeft, ArrowRight};

use crate::components::button::{Button, ButtonVariant};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View};
use crate::widgets::Tip;

/// The steps, in order: the primary destinations, as the sidebar lists them. An estate
/// without a Presets destination skips that one.
const STEPS: [View; 8] = View::PRIMARY;

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

/// What a step is for, in one sentence; empty for a view that is not a step.
pub fn purpose(step: View) -> &'static str {
    match step {
        View::Overview => "See what the estate is and what it still owes.",
        View::Presets => "Choose the presets this organization is built from.",
        View::Packs => "Take the packs this estate is built from.",
        View::Decisions => "Answer what the estate's presets and packs ask.",
        View::Estate => "Set any value of the estate file the questions do not ask for.",
        View::Apply => "Check who is signed in, then check, plan and apply.",
        View::Security => "See which security checks the estate's own resources fail.",
        View::Documents => "Make the documents the customer receives.",
        _ => "",
    }
}

/// The bar at the window's foot while a step is showing: at its right edge a ghost button
/// "← <step>" to the step before and a primary button "<step> →" to the next, each with
/// what that step is for as its tooltip, opening upward. The first step has no previous,
/// the last no next. Nothing on a view that is not a step, and nothing without an open
/// estate.
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
                Tip { text: purpose(previous),
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| app.nav().set(previous),
                        ArrowLeft { size: 16 }
                        "{previous.label()}"
                    }
                }
            }
            if let Some(next) = next {
                Tip { text: purpose(next),
                    Button {
                        onclick: move |_| app.nav().set(next),
                        "{next.label()}"
                        ArrowRight { size: 16 }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk is the sidebar's order, Estate between the questions and Apply; an estate
    /// without presets goes from the Overview to its packs.
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
                View::Estate,
                View::Apply,
                View::Security,
                View::Documents
            ]
        );
        // Estate is the step between the questions and Apply
        assert_eq!(next_after(View::Decisions, true), Some(View::Estate));
        assert_eq!(next_after(View::Estate, false), Some(View::Apply));
        // not a step, and the last step
        assert_eq!(next_after(View::Settings, true), None);
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
                View::Estate,
                View::Apply,
                View::Security,
                View::Documents
            ]
        );
        // not a step, and the first step
        assert_eq!(previous_before(View::Agent, true), None);
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

    /// Every step says what it is for — the path bar shows it as the tooltip of the button
    /// that goes there, back to the Overview included — and is a primary destination.
    #[test]
    fn every_step_has_a_purpose_and_is_in_the_sidebar() {
        for step in &STEPS {
            assert!(!purpose(*step).is_empty(), "{step:?}");
            assert!(View::PRIMARY.contains(step), "{step:?}");
        }
    }
}
