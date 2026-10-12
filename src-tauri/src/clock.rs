use crate::message::MessageError;
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock {
    fn now_millis(&self) -> Result<i64, MessageError>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> Result<i64, MessageError> {
        unix_millis(SystemTime::now())
    }
}

fn unix_millis(time: SystemTime) -> Result<i64, MessageError> {
    let elapsed = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MessageError::InvalidSystemTime)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| MessageError::InvalidSystemTime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn uses_milliseconds_and_rejects_times_before_epoch() {
        assert_eq!(
            unix_millis(UNIX_EPOCH + Duration::from_millis(1234)).unwrap(),
            1234
        );
        assert!(matches!(
            unix_millis(UNIX_EPOCH - Duration::from_millis(1)),
            Err(MessageError::InvalidSystemTime)
        ));
    }
}
