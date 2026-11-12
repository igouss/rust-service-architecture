use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub min: Duration,
    pub p50: Duration,
    pub p90: Duration,
}

fn nearest_rank(sorted: &[Duration], percent: usize) -> Option<Duration> {
    let rank: usize = (sorted.len() * percent).div_ceil(100);
    sorted.get(rank.checked_sub(1)?).copied()
}

pub fn summarize(mut samples: Vec<Duration>) -> Option<Summary> {
    samples.sort();
    Some(Summary {
        min: samples.first().copied()?,
        p50: nearest_rank(&samples, 50)?,
        p90: nearest_rank(&samples, 90)?,
    })
}

pub fn after_warm_ups(mut samples: Vec<Duration>, warm_ups: usize) -> Vec<Duration> {
    samples.split_off(warm_ups.min(samples.len()))
}

pub fn micros(duration: Duration) -> f64 {
    duration.as_nanos() as f64 / 1_000.0
}

pub fn ratio(numerator: Duration, denominator: Duration) -> f64 {
    numerator.as_nanos() as f64 / denominator.as_nanos() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn micro(value: u64) -> Duration {
        Duration::from_micros(value)
    }

    #[test]
    fn no_samples_have_no_summary() {
        assert_eq!(summarize(Vec::new()), None);
    }

    #[test]
    fn one_sample_is_its_own_min_median_and_p90() {
        assert_eq!(
            summarize(vec![micro(7)]),
            Some(Summary {
                min: micro(7),
                p50: micro(7),
                p90: micro(7)
            })
        );
    }

    #[test]
    fn two_samples_take_the_lower_as_median_and_the_upper_as_p90() {
        assert_eq!(
            summarize(vec![micro(9), micro(3)]),
            Some(Summary {
                min: micro(3),
                p50: micro(3),
                p90: micro(9)
            })
        );
    }

    #[test]
    fn thirty_unsorted_samples_summarize_by_nearest_rank() {
        let samples: Vec<Duration> = (1..=30).rev().map(micro).collect();
        assert_eq!(
            summarize(samples),
            Some(Summary {
                min: micro(1),
                p50: micro(15),
                p90: micro(27)
            })
        );
    }

    #[test]
    fn with_no_warm_ups_every_sample_is_kept() {
        assert_eq!(after_warm_ups(vec![micro(1), micro(2)], 0), vec![micro(1), micro(2)]);
    }

    #[test]
    fn one_warm_up_drops_the_first_sample() {
        assert_eq!(after_warm_ups(vec![micro(1), micro(2), micro(3)], 1), vec![micro(2), micro(3)]);
    }

    #[test]
    fn many_warm_ups_drop_the_first_samples_in_order() {
        assert_eq!(after_warm_ups(vec![micro(1), micro(2), micro(3)], 2), vec![micro(3)]);
    }

    #[test]
    fn more_warm_ups_than_samples_keep_nothing() {
        assert_eq!(after_warm_ups(vec![micro(1)], 5), Vec::new());
    }

    #[test]
    fn a_duration_reads_as_microseconds() {
        assert_eq!(micros(Duration::from_nanos(250_500)), 250.5);
    }

    #[test]
    fn a_ratio_divides_the_first_duration_by_the_second() {
        assert_eq!(ratio(micro(30), micro(20)), 1.5);
    }
}
