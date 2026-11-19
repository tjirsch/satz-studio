//! The path through an estate, as the rail orders it: what the estate is, its components,
//! their questions, its way to the organisation, its documents. Each step's view ends
//! with a bar that names the next one and goes there
//! ([ADR 0028](../../../../docs/adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)).
//! The Estate destination is not a step: it is the estate file itself, for whoever wants
//! a value the questions do not ask for.

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Card, CardVariant, Icon};
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

/// What the next step is for, in one sentence.
pub fn purpose(step: View) -> &'static str {
    match step {
        View::Presets => "Choose the presets this organisation is built from.",
        View::Packs => "Take the packs this estate is built from.",
        View::Decisions => "Answer what the estate's presets and packs ask.",
        View::Apply => "Check who is signed in, then check, plan and apply.",
        View::Documents => "Make the documents that are handed over.",
        _ => "",
    }
}

/// The bar at the end of a step's view: the next step's purpose and a button that goes
/// there. Nothing at the last step.
#[component]
pub fn NextStep(from: View) -> Element {
    let app = use_context::<Store<AppStore>>();
    let presets = app
        .estate()
        .model()
        .read()
        .as_deref()
        .is_none_or(|m| m.packs.has_presets());
    let Some(next) = next_after(from, presets) else {
        return rsx! {};
    };
    rsx! {
        Card { variant: CardVariant::Filled, class: "next-step",
            Icon { name: next.icon(), size: 22 }
            span { class: "next-step__text", "{purpose(next)}" }
            span { class: "grow" }
            Button {
                variant: ButtonVariant::Filled,
                icon: "arrow_forward",
                onclick: move |_| app.nav().set(next),
                "Next: {next.label()}"
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

    /// Every step that can be gone to says what it is for, and is a primary destination.
    #[test]
    fn every_next_step_has_a_purpose_and_is_in_the_rail() {
        for step in &STEPS[1..] {
            assert!(!purpose(*step).is_empty(), "{step:?}");
            assert!(View::PRIMARY.contains(step), "{step:?}");
        }
    }
}
