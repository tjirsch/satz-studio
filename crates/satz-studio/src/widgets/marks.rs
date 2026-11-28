//! The marks: what a question or a param carries beside its name, each the library's
//! `Badge` with a Lucide icon and a short word, and the whole sentence in its tooltip
//! (`Tip`). One mark has one look wherever it stands — a Decisions card, an Estate →
//! Params row, a Packs row and its expands: what warns is `Destructive` (one-way, a high
//! blast, an unmet read), the rest `Outline`.

use dioxus::prelude::*;
use dioxus_icons::lucide::{DoorOpen, History, MessageCircleQuestionMark, Sparkles, TriangleAlert};
use satz_studio_core::satz::reports::{Blast, Reversal};

use crate::components::badge::{Badge, BadgeVariant};
use crate::widgets::Tip;

/// The word of the reversal mark: what changing the answer later is.
pub fn reversal_label(r: Reversal) -> &'static str {
    match r {
        Reversal::Edit => "edit",
        Reversal::StateSurgery => "state surgery",
        Reversal::Recreate => "recreate",
    }
}

/// The word of the blast mark: what changing the answer later costs.
pub fn blast_label(b: Blast) -> &'static str {
    match b {
        Blast::None => "blast: none",
        Blast::Low => "blast: low",
        Blast::High => "blast: high",
    }
}

/// What changing the answer later does to the estate, in satz's words for `reversal`.
pub fn reversal_tip(r: Reversal) -> &'static str {
    match r {
        Reversal::Edit => "Changing this answer later is an edit: one line, one apply.",
        Reversal::StateSurgery => {
            "Changing this answer later is state surgery: a rename plus state mv, and whatever the old name is wired into. Done with care."
        }
        Reversal::Recreate => {
            "Changing this answer later destroys and recreates resources: globally unique ids, mail addresses, buckets."
        }
    }
}

/// What changing the answer later costs the running organization, in satz's words for
/// `blast`. Independent of the reversal: one boolean to reverse can cut every existing
/// SSH path.
pub fn blast_tip(b: Blast) -> &'static str {
    match b {
        Blast::None => "Changing this answer later costs the running organization nothing.",
        Blast::Low => "Changing this answer later costs the running organization little.",
        Blast::High => {
            "Changing this answer later hits the running organization hard: what runs on the old answer can stop working."
        }
    }
}

/// What the one-way mark means.
pub const ONE_WAY_TIP: &str = "One-way door: changing this answer later is a recreate or hits a high blast radius. Decide it with the reason in view.";

/// "recreate", "state surgery" or "edit" (`History`), outline.
#[component]
pub fn ReversalBadge(reversal: Reversal) -> Element {
    rsx! {
        Tip { text: reversal_tip(reversal),
            Badge { variant: BadgeVariant::Outline,
                History { size: 12 }
                "{reversal_label(reversal)}"
            }
        }
    }
}

/// "blast: high", "blast: low" or "blast: none" (`Sparkles`), destructive when high.
#[component]
pub fn BlastBadge(blast: Blast) -> Element {
    rsx! {
        Tip { text: blast_tip(blast),
            Badge { variant: if blast == Blast::High { BadgeVariant::Destructive } else { BadgeVariant::Outline },
                Sparkles { size: 12 }
                "{blast_label(blast)}"
            }
        }
    }
}

/// "one-way" (`DoorOpen`), destructive.
#[component]
pub fn OneWayBadge() -> Element {
    rsx! {
        Tip { text: ONE_WAY_TIP,
            Badge { variant: BadgeVariant::Destructive,
                DoorOpen { size: 12 }
                "one-way"
            }
        }
    }
}

/// "asked" (`MessageCircleQuestionMark`), outline: a question asks for the param; `tip` is
/// its prompt and its `why`.
#[component]
pub fn AskedBadge(tip: String) -> Element {
    rsx! {
        Tip { text: tip,
            Badge { variant: BadgeVariant::Outline,
                MessageCircleQuestionMark { size: 12 }
                "asked"
            }
        }
    }
}

/// "unmet read" (`TriangleAlert`), destructive: the estate does not give a value the pack
/// reads; `tip` says which and why.
#[component]
pub fn UnmetBadge(tip: String) -> Element {
    rsx! {
        Tip { text: tip,
            Badge { variant: BadgeVariant::Destructive,
                TriangleAlert { size: 12 }
                "unmet read"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each mark's word is short and its own; the sentence is the tooltip's.
    #[test]
    fn every_mark_has_a_short_word_of_its_own() {
        let reversals = [Reversal::Edit, Reversal::StateSurgery, Reversal::Recreate];
        let blasts = [Blast::None, Blast::Low, Blast::High];
        let words: Vec<&str> = reversals
            .into_iter()
            .map(reversal_label)
            .chain(blasts.into_iter().map(blast_label))
            .collect();
        assert_eq!(
            words,
            [
                "edit",
                "state surgery",
                "recreate",
                "blast: none",
                "blast: low",
                "blast: high"
            ]
        );
        for r in reversals {
            assert!(reversal_tip(r).len() > reversal_label(r).len());
        }
        for b in blasts {
            assert!(blast_tip(b).len() > blast_label(b).len());
        }
    }
}
