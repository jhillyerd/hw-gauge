use std::collections::VecDeque;
use std::fmt::Display;

#[derive(Debug)]
pub struct Averager {
    samples: VecDeque<f64>,
    max_samples: usize,
}

impl Display for Averager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[avg {:.04} over {} samples]",
            self.average().unwrap_or(0.0),
            self.samples.len(),
        )
    }
}

impl Averager {
    /// Creates a new Averager, tracking up to max_samples values.
    pub fn new(max_samples: usize) -> Self {
        assert!(max_samples > 1, "max_samples must be at least 2");
        Averager {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    pub fn average(&self) -> Option<f64> {
        let len = self.samples.len();
        if len == 0 {
            return None;
        }

        Some(self.samples.iter().sum::<f64>() / (self.samples.len() as f64))
    }

    pub fn add_sample(&mut self, sample: f64) {
        if self.samples.len() == self.max_samples {
            self.samples.pop_front();
        }

        self.samples.push_back(sample);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_is_empty() {
        let avg = Averager::new(2);

        let actual = avg.average();

        assert_eq!(actual, None);
    }

    #[test]
    fn returns_identity() {
        let mut avg = Averager::new(2);
        avg.add_sample(5.0);
        avg.add_sample(5.0);

        let actual = avg.average();

        assert_eq!(actual, Some(5.0));
    }

    #[test]
    fn returns_average_before_max_samples() {
        let mut avg = Averager::new(20);
        avg.add_sample(5.0);
        avg.add_sample(15.0);
        avg.add_sample(5.0);
        avg.add_sample(15.0);

        let actual = avg.average();

        assert_eq!(actual, Some(40.0 / 4.0));
    }

    #[test]
    fn returns_average_at_max_samples() {
        let mut avg = Averager::new(4);
        avg.add_sample(5.0);
        avg.add_sample(15.0);
        avg.add_sample(5.0);
        avg.add_sample(15.0);

        let actual = avg.average();

        assert_eq!(actual, Some(40.0 / 4.0));
    }

    #[test]
    fn returns_average_beyond_max_samples() {
        let mut avg = Averager::new(4);
        avg.add_sample(5.0);
        avg.add_sample(15.0);
        avg.add_sample(5.0);
        avg.add_sample(15.0);
        avg.add_sample(100.0);

        let actual = avg.average();

        let expected = (15.0 + 5.0 + 15.0 + 100.0) / 4.0;
        assert_eq!(actual, Some(expected));
    }
}
