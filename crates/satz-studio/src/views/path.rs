//! The path through an estate, as the rail orders it: what the estate is, its components,
//! their questions, its way to the organisation, its documents. Each step's view ends
//! with a bar that names the next one and goes there
//! ([ADR 0028](../../../../docs/adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)).
//! The Estate destination is not a step: it is the estate file itself, for whoever wants
//! a value the questions do not ask for.

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Card, CardVariant, Icon};
use crate::state::{AppStore, AppStoreStoreExt, View};

/// The steps, in order, for every estate.
const STEPS: [View; 5] = [
    View::Overview,
    View::Components,
    View::Decisions,
    View::Apply,
    View::Documents,
];

/// The step after `from`, or `None` for the last step and for a view that is not one.
pub fn next_after(from: View) -> Option<View> {
    let at = STEPS.iter().position(|v| *v == from)?;
    STEPS.get(at + 1).copied()
}

/// What the next step is for, in one sentence.
pub fn purpose(step: View) -> &'static str {
    match step {
        View::Components => "Choose the components this estate is built from.",
        View::Decisions => "Answer what the estate and its components ask.",
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
    let Some(next) = next_after(from) else {
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

    /// The walk is the rail's order without Estate, the same for every estate.
    #[test]
    fn the_steps_follow_each_other() {
        let mut at = View::Overview;
        let mut walk = vec![at];
        while let Some(next) = next_after(at) {
            walk.push(next);
            at = next;
        }
        assert_eq!(walk, STEPS);
        // not a step, and the last step
        assert_eq!(next_after(View::Estate), None);
        assert_eq!(next_after(View::Documents), None);
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
