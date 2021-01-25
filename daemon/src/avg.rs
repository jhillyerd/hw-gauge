use std::fmt::Display;

#[derive(Debug)]
pub struct Averager {
    value: Option<f64>,
    sample_count: usize,
    max_samples: usize,
}

impl Display for Averager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[avg {:.04} over {} samples]",
            self.value.unwrap_or(0.0),
            self.sample_count
        )
    }
}

impl Averager {
    /// Creates a new Averager, tracking up to max_samples values.
    pub fn new(max_samples: usize) -> Self {
        assert!(max_samples > 1, "max_samples must be at least 2");
        Averager {
            value: None,
            sample_count: 0,
            max_samples,
        }
    }

    pub fn average(&self) -> Option<f64> {
        self.value
    }

    pub fn add_sample(&mut self, sample: f64) {
        if let Some(value) = self.value {
            if self.sample_count < self.max_samples {
                self.sample_count += 1;
            }
            let scaled = value * ((self.sample_count - 1) as f64);
            self.value = Some((scaled + sample) / (self.sample_count as f64));
        } else {
            self.value = Some(sample);
            self.sample_count = 1;
        }
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
        let expected = ((10.0 * 3.0) + 100.0) / 4.0;

        assert_eq!(actual, Some(expected));
    }
}
