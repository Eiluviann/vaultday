//! The close-out's allocation: a day's logged hours spread over the stories
//! actually worked that day, in whole minutes.
//!
//! The rules, each a property test below:
//!
//! - Only the stories given for the day get time; a story that was not
//!   touched is never in the input, so it never gets a minute.
//! - No story gets more than its remaining estimate (its unused budget).
//! - The day sums to its target unless the stories' remaining budget is
//!   smaller; then each story gets all it has left.
//! - Otherwise the target is shared in proportion to the remaining budgets,
//!   so unused budget is drawn down first: a story with more left never gets
//!   less than one with less left, and each share is within one minute of
//!   its exact proportional share.
//! - Rounding of the last minutes: every story gets the whole minutes of its
//!   exact share; the minutes left over go one each to the stories with the
//!   largest fractional remainder, ties to the larger remaining budget, then
//!   to the smaller story number. The result does not depend on input order.
//!
//! Tracked minutes do not weigh: tracked time is private and real, logged
//! time is the allocation. They come along so a caller can show them.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use jiff::civil::Date;

use crate::task::StoryNumber;

/// One story worked on a day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryDay {
    /// The story.
    pub story: StoryNumber,
    /// Minutes tracked on it that day; shown, never weighed.
    pub tracked_minutes: u32,
    /// Minutes of its estimate not yet allocated; 0 for a story without one.
    pub remaining_minutes: u32,
}

/// Minutes allocated to one story.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Allotment {
    /// The story.
    pub story: StoryNumber,
    /// Whole minutes.
    pub minutes: u32,
}

/// One day of a period to allocate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayWork {
    /// The day.
    pub date: Date,
    /// Minutes to allocate: the workspace's `hours_per_day`.
    pub target_minutes: u32,
    /// The stories touched that day with their tracked minutes.
    pub touched: Vec<(StoryNumber, u32)>,
    /// An allocation already set for the day (confirmed or adjusted by the
    /// user); it is kept as it is and drawn from the budgets.
    pub fixed: Option<Vec<Allotment>>,
}

/// One allocated day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayAllocation {
    /// The day.
    pub date: Date,
    /// Minutes to allocate.
    pub target_minutes: u32,
    /// The stories of the day, by story number, with what each gets.
    pub allotments: Vec<Allotment>,
    /// Whether the allotments came from [`DayWork::fixed`].
    pub is_fixed: bool,
}

impl DayAllocation {
    /// The minutes allocated.
    pub fn total_minutes(&self) -> u32 {
        self.allotments
            .iter()
            .map(|allotment| allotment.minutes)
            .sum()
    }
}

/// Spreads `target_minutes` over `stories` by the rules of this module.
/// Returns every given story once, by story number; a story given twice is
/// merged (tracked minutes summed, the larger remaining kept).
pub fn allocate_day(target_minutes: u32, stories: &[StoryDay]) -> Vec<Allotment> {
    let stories = merged(stories);
    let total_remaining: u64 = stories
        .iter()
        .map(|story| u64::from(story.remaining_minutes))
        .sum();
    let target = u64::from(target_minutes);
    if total_remaining <= target {
        return stories
            .into_iter()
            .map(|story| Allotment {
                story: story.story,
                minutes: story.remaining_minutes,
            })
            .collect();
    }
    let mut shares: Vec<(StoryDay, u64, u64)> = stories
        .into_iter()
        .map(|story| {
            let exact = target * u64::from(story.remaining_minutes);
            let whole = exact / total_remaining;
            let fraction = exact % total_remaining;
            (story, whole, fraction)
        })
        .collect();
    let given: u64 = shares.iter().map(|(_, whole, _)| whole).sum();
    let leftover = usize::try_from(target - given).unwrap_or(usize::MAX);
    let mut order: Vec<usize> = (0..shares.len()).collect();
    order.sort_by_key(|&index| {
        let (story, _, fraction) = &shares[index];
        (
            Reverse(*fraction),
            Reverse(story.remaining_minutes),
            story.story.clone(),
        )
    });
    for &index in order.iter().take(leftover) {
        shares[index].1 += 1;
    }
    shares
        .into_iter()
        .map(|(story, minutes, _)| Allotment {
            story: story.story,
            // At most the story's remaining minutes, a `u32`.
            minutes: u32::try_from(minutes).unwrap_or(u32::MAX),
        })
        .collect()
}

/// The day re-balanced after the user set `story` to `minutes`: that story
/// keeps `minutes` (at most its remaining and the target) and the rest of
/// the target is spread over the other stories by [`allocate_day`]. The day
/// keeps its total unless the others' remaining budget is too small.
pub fn rebalance_day(
    target_minutes: u32,
    stories: &[StoryDay],
    story: &StoryNumber,
    minutes: u32,
) -> Vec<Allotment> {
    let stories = merged(stories);
    let Some(chosen) = stories.iter().find(|candidate| &candidate.story == story) else {
        return allocate_day(target_minutes, &stories);
    };
    let kept = minutes.min(chosen.remaining_minutes).min(target_minutes);
    let others: Vec<StoryDay> = stories
        .iter()
        .filter(|candidate| &candidate.story != story)
        .cloned()
        .collect();
    let mut allotments = allocate_day(target_minutes - kept, &others);
    allotments.push(Allotment {
        story: story.clone(),
        minutes: kept,
    });
    allotments.sort();
    allotments
}

/// Allocates the days in date order, each drawing on the budgets the days
/// before it left. `remaining_at_start` is each story's unused budget when
/// the first day begins; a story missing from it has none.
pub fn allocate_days(
    days: &[DayWork],
    remaining_at_start: &BTreeMap<StoryNumber, u32>,
) -> Vec<DayAllocation> {
    let mut remaining = remaining_at_start.clone();
    let mut days: Vec<&DayWork> = days.iter().collect();
    days.sort_by_key(|day| day.date);
    days.into_iter()
        .map(|day| {
            let (allotments, is_fixed) = match &day.fixed {
                Some(fixed) => {
                    let mut fixed = fixed.clone();
                    fixed.sort();
                    (fixed, true)
                }
                None => {
                    let stories: Vec<StoryDay> = day
                        .touched
                        .iter()
                        .map(|(story, tracked)| StoryDay {
                            story: story.clone(),
                            tracked_minutes: *tracked,
                            remaining_minutes: remaining.get(story).copied().unwrap_or(0),
                        })
                        .collect();
                    (allocate_day(day.target_minutes, &stories), false)
                }
            };
            for allotment in &allotments {
                if let Some(left) = remaining.get_mut(&allotment.story) {
                    *left = left.saturating_sub(allotment.minutes);
                }
            }
            DayAllocation {
                date: day.date,
                target_minutes: day.target_minutes,
                allotments,
                is_fixed,
            }
        })
        .collect()
}

/// The stories by number, each once.
fn merged(stories: &[StoryDay]) -> Vec<StoryDay> {
    let mut by_story: BTreeMap<StoryNumber, StoryDay> = BTreeMap::new();
    for story in stories {
        by_story
            .entry(story.story.clone())
            .and_modify(|known| {
                known.tracked_minutes = known.tracked_minutes.saturating_add(story.tracked_minutes);
                known.remaining_minutes = known.remaining_minutes.max(story.remaining_minutes);
            })
            .or_insert_with(|| story.clone());
    }
    by_story.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use proptest::prelude::*;

    fn day(entries: &[(&str, u32, u32)]) -> Vec<StoryDay> {
        entries
            .iter()
            .map(|(story, tracked, remaining)| StoryDay {
                story: StoryNumber::new(*story),
                tracked_minutes: *tracked,
                remaining_minutes: *remaining,
            })
            .collect()
    }

    fn minutes(allotments: &[Allotment]) -> Vec<(&str, u32)> {
        allotments
            .iter()
            .map(|allotment| (allotment.story.as_str(), allotment.minutes))
            .collect()
    }

    #[test]
    fn the_day_is_shared_by_remaining_budget() {
        // 390 minutes (6.5 h) over budgets of 1800, 1200 and 90 minutes.
        let stories = day(&[("A-2", 100, 1200), ("A-1", 185, 1800), ("A-3", 75, 90)]);
        assert_eq!(
            minutes(&allocate_day(390, &stories)),
            [("A-1", 227), ("A-2", 152), ("A-3", 11)]
        );
    }

    #[test]
    fn a_small_budget_is_used_up_and_the_day_falls_short() {
        let stories = day(&[("A-1", 300, 120), ("A-2", 60, 30)]);
        assert_eq!(
            minutes(&allocate_day(390, &stories)),
            [("A-1", 120), ("A-2", 30)]
        );
        assert_eq!(
            minutes(&allocate_day(390, &day(&[("A-1", 60, 0)]))),
            [("A-1", 0)]
        );
        assert!(allocate_day(390, &[]).is_empty());
    }

    #[test]
    fn the_last_minute_goes_by_remainder_then_budget_then_number() {
        // Three equal budgets: 10 minutes leave one over, which goes to the
        // smallest story number.
        let stories = day(&[("B-3", 0, 60), ("B-1", 0, 60), ("B-2", 0, 60)]);
        assert_eq!(
            minutes(&allocate_day(10, &stories)),
            [("B-1", 4), ("B-2", 3), ("B-3", 3)]
        );
    }

    #[test]
    fn adjusting_one_story_keeps_the_total() {
        let stories = day(&[("A-1", 0, 1800), ("A-2", 0, 1200), ("A-3", 0, 90)]);
        let adjusted = rebalance_day(390, &stories, &StoryNumber::new("A-3"), 60);
        assert_eq!(
            minutes(&adjusted),
            [("A-1", 198), ("A-2", 132), ("A-3", 60)]
        );
        // Above its remaining budget the story is held at it.
        let capped = rebalance_day(390, &stories, &StoryNumber::new("A-3"), 500);
        assert_eq!(capped.iter().map(|a| a.minutes).sum::<u32>(), 390);
        assert_eq!(capped[2].minutes, 90);
        // An unknown story changes nothing.
        assert_eq!(
            rebalance_day(390, &stories, &StoryNumber::new("Z-9"), 10),
            allocate_day(390, &stories)
        );
    }

    #[test]
    fn days_draw_down_the_budget_in_order() {
        let story = StoryNumber::new("A-1");
        let other = StoryNumber::new("A-2");
        let days = [
            DayWork {
                date: date(2026, 10, 20),
                target_minutes: 390,
                touched: vec![(story.clone(), 200)],
                fixed: None,
            },
            DayWork {
                date: date(2026, 10, 19),
                target_minutes: 390,
                touched: vec![(story.clone(), 100), (other.clone(), 50)],
                fixed: Some(vec![
                    Allotment {
                        story: story.clone(),
                        minutes: 300,
                    },
                    Allotment {
                        story: other.clone(),
                        minutes: 90,
                    },
                ]),
            },
        ];
        let start = BTreeMap::from([(story.clone(), 500), (other, 1000)]);
        let allocated = allocate_days(&days, &start);
        assert_eq!(allocated[0].date, date(2026, 10, 19));
        assert!(allocated[0].is_fixed);
        assert_eq!(minutes(&allocated[1].allotments), [("A-1", 200)]);
        assert_eq!(allocated[1].total_minutes(), 200);
    }

    fn arbitrary_day() -> impl Strategy<Value = (u32, Vec<StoryDay>)> {
        (
            0_u32..=1440,
            proptest::collection::vec((0_u32..=600, 0_u32..=3000), 0..8),
        )
            .prop_map(|(target, entries)| {
                let stories = entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, (tracked, remaining))| StoryDay {
                        story: StoryNumber::new(format!("S-{index}")),
                        tracked_minutes: tracked,
                        remaining_minutes: remaining,
                    })
                    .collect();
                (target, stories)
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn it_sums_to_the_target_unless_the_budget_is_smaller((target, stories) in arbitrary_day()) {
            let total: u32 = allocate_day(target, &stories).iter().map(|a| a.minutes).sum();
            let budget: u32 = stories.iter().map(|s| s.remaining_minutes).sum();
            prop_assert_eq!(total, target.min(budget));
        }

        #[test]
        fn no_story_goes_over_its_remaining((target, stories) in arbitrary_day()) {
            for allotment in allocate_day(target, &stories) {
                let story = stories.iter().find(|s| s.story == allotment.story).unwrap();
                prop_assert!(allotment.minutes <= story.remaining_minutes);
            }
        }

        #[test]
        fn only_the_stories_worked_get_time((target, stories) in arbitrary_day()) {
            let allotments = allocate_day(target, &stories);
            prop_assert_eq!(allotments.len(), stories.len());
            for allotment in &allotments {
                prop_assert!(stories.iter().any(|s| s.story == allotment.story));
            }
        }

        #[test]
        fn more_unused_budget_never_gets_less((target, stories) in arbitrary_day()) {
            let allotments = allocate_day(target, &stories);
            for a in &allotments {
                for b in &allotments {
                    let ra = stories.iter().find(|s| s.story == a.story).unwrap().remaining_minutes;
                    let rb = stories.iter().find(|s| s.story == b.story).unwrap().remaining_minutes;
                    if ra > rb {
                        prop_assert!(a.minutes >= b.minutes, "{a:?} {b:?}");
                    }
                }
            }
        }

        #[test]
        fn each_share_is_within_a_minute_of_its_weight((target, stories) in arbitrary_day()) {
            let budget: u64 = stories.iter().map(|s| u64::from(s.remaining_minutes)).sum();
            prop_assume!(budget > u64::from(target));
            for allotment in allocate_day(target, &stories) {
                let remaining = stories.iter().find(|s| s.story == allotment.story).unwrap().remaining_minutes;
                // |minutes - target * remaining / budget| < 1, in whole numbers.
                let exact = u64::from(target) * u64::from(remaining);
                let scaled = u64::from(allotment.minutes) * budget;
                prop_assert!(scaled.abs_diff(exact) < budget);
            }
        }

        #[test]
        fn it_does_not_depend_on_input_order((target, stories) in arbitrary_day()) {
            let mut reversed = stories.clone();
            reversed.reverse();
            prop_assert_eq!(allocate_day(target, &stories), allocate_day(target, &reversed));
        }

        #[test]
        fn rebalancing_keeps_the_total_when_the_budget_allows(
            (target, stories) in arbitrary_day(),
            pick in 0_usize..8,
            wanted in 0_u32..=1440,
        ) {
            prop_assume!(!stories.is_empty());
            let chosen = &stories[pick % stories.len()];
            let budget: u32 = stories.iter().map(|s| s.remaining_minutes).sum();
            let others: u32 = budget - chosen.remaining_minutes;
            let kept = wanted.min(chosen.remaining_minutes).min(target);
            let adjusted = rebalance_day(target, &stories, &chosen.story, wanted);
            let total: u32 = adjusted.iter().map(|a| a.minutes).sum();
            prop_assert_eq!(total, kept + (target - kept).min(others));
            for allotment in &adjusted {
                let story = stories.iter().find(|s| s.story == allotment.story).unwrap();
                prop_assert!(allotment.minutes <= story.remaining_minutes);
            }
        }
    }
}
