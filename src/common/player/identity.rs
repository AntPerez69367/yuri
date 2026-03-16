use crate::common::types::Point;

/// Core identity — ID, name, login info, saved position. Rarely mutated after init.
#[derive(Debug, Clone)]
pub struct PlayerIdentity {
    pub id: u32,
    pub name: String,
    pub pass: String,
    pub f1name: String,
    pub title: String,
    pub ipaddress: String,
    pub gm_level: i8,
    pub sex: i8,
    pub map_server: i32,
    pub dest_pos: Point,
    pub last_pos: Point,
}

impl Default for PlayerIdentity {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            pass: String::new(),
            f1name: String::new(),
            title: String::new(),
            ipaddress: String::new(),
            gm_level: 0,
            sex: 0,
            map_server: 0,
            dest_pos: Point::new(0, 0, 0),
            last_pos: Point::new(0, 0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_identity() {
        let id = PlayerIdentity::default();
        assert_eq!(id.id, 0);
        assert!(id.name.is_empty());
        assert_eq!(id.dest_pos, Point::new(0, 0, 0));
    }
}
