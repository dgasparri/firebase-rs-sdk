use crate::firestore::error::{invalid_argument, FirestoreResult};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    latitude: f64,
    longitude: f64,
}

impl GeoPoint {
    pub fn new(latitude: f64, longitude: f64) -> FirestoreResult<Self> {
        if !(-90.0..=90.0).contains(&latitude) {
            return Err(invalid_argument("Latitude must be between -90 and 90 degrees."));
        }
        if !(-180.0..=180.0).contains(&longitude) {
            return Err(invalid_argument("Longitude must be between -180 and 180 degrees."));
        }
        Ok(Self { latitude, longitude })
    }

    pub fn latitude(&self) -> f64 {
        self.latitude
    }

    pub fn longitude(&self) -> f64 {
        self.longitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_coordinates() {
        let point = GeoPoint::new(10.0, 20.0).unwrap();
        assert_eq!(point.latitude(), 10.0);
        assert_eq!(point.longitude(), 20.0);
    }

    #[test]
    fn invalid_latitude() {
        let err = GeoPoint::new(100.0, 0.0).unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }
}
