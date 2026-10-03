#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRectangle {
    pub fn intersects(&self, other: Self) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    pub fn expanded_by(&self, margin: f64) -> Self {
        Self {
            x: self.x - margin,
            y: self.y - margin,
            width: self.width + margin * 2.0,
            height: self.height + margin * 2.0,
        }
    }
}

pub fn place_hud_outside_focused_field(
    screen: ScreenRectangle,
    width: f64,
    height: f64,
    focused_field: Option<ScreenRectangle>,
) -> Option<ScreenRectangle> {
    let margin = 16.0;
    if width <= 0.0
        || height <= 0.0
        || width + margin * 2.0 > screen.width
        || height + margin * 2.0 > screen.height
    {
        return None;
    }
    let left = screen.x + margin;
    let right = screen.x + screen.width - width - margin;
    let centre = screen.x + (screen.width - width) / 2.0;
    let top = screen.y + screen.height - height - margin;
    let bottom = screen.y + margin;
    let forbidden = focused_field.map(|field| field.expanded_by(margin));
    [
        (centre, top),
        (left, top),
        (right, top),
        (centre, bottom),
        (left, bottom),
        (right, bottom),
    ]
    .into_iter()
    .map(|(x, y)| ScreenRectangle {
        x,
        y,
        width,
        height,
    })
    .find(|candidate| forbidden.is_none_or(|field| !candidate.intersects(field)))
}

pub fn place_hud_above_caret(
    screen: ScreenRectangle,
    width: f64,
    height: f64,
    caret: ScreenRectangle,
) -> Option<ScreenRectangle> {
    let edge_margin = 16.0;
    let caret_gap = 16.0;
    if width <= 0.0
        || height <= 0.0
        || width + edge_margin * 2.0 > screen.width
        || height + edge_margin * 2.0 > screen.height
    {
        return None;
    }
    let x = (caret.x + caret.width / 2.0 - width / 2.0).clamp(
        screen.x + edge_margin,
        screen.x + screen.width - width - edge_margin,
    );
    let above = caret.y + caret.height + caret_gap;
    let below = caret.y - height - caret_gap;
    [above, below]
        .into_iter()
        .filter(|y| {
            *y >= screen.y + edge_margin && *y + height <= screen.y + screen.height - edge_margin
        })
        .map(|y| ScreenRectangle {
            x,
            y,
            width,
            height,
        })
        .find(|hud| !hud.intersects(caret.expanded_by(caret_gap - 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_rectangle() -> ScreenRectangle {
        ScreenRectangle {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 800.0,
        }
    }

    #[test]
    fn bottom_composer_does_not_overlap_the_hud() {
        let field = ScreenRectangle {
            x: 200.0,
            y: 30.0,
            width: 800.0,
            height: 200.0,
        };
        let hud =
            place_hud_outside_focused_field(screen_rectangle(), 400.0, 70.0, Some(field)).unwrap();
        assert!(!hud.intersects(field.expanded_by(16.0)));
        assert_eq!(hud.y, 714.0);
    }

    #[test]
    fn top_field_moves_the_hud_to_the_bottom() {
        let field = ScreenRectangle {
            x: 0.0,
            y: 600.0,
            width: 1200.0,
            height: 200.0,
        };
        let hud =
            place_hud_outside_focused_field(screen_rectangle(), 400.0, 70.0, Some(field)).unwrap();
        assert_eq!(hud.y, 16.0);
        assert!(!hud.intersects(field.expanded_by(16.0)));
    }

    #[test]
    fn a_full_screen_field_leaves_only_the_tray_indicator() {
        assert!(place_hud_outside_focused_field(
            screen_rectangle(),
            400.0,
            70.0,
            Some(screen_rectangle())
        )
        .is_none());
    }

    #[test]
    fn unavailable_field_bounds_use_the_top_edge() {
        let hud = place_hud_outside_focused_field(screen_rectangle(), 400.0, 70.0, None).unwrap();
        assert_eq!((hud.x, hud.y), (400.0, 714.0));
    }

    #[test]
    fn secondary_monitor_offsets_are_preserved() {
        let screen = ScreenRectangle {
            x: -1600.0,
            y: 900.0,
            width: 1200.0,
            height: 800.0,
        };
        let hud = place_hud_outside_focused_field(screen, 400.0, 70.0, None).unwrap();
        assert_eq!((hud.x, hud.y), (-1200.0, 1614.0));
    }

    #[test]
    fn undersized_screens_do_not_place_the_hud_offscreen() {
        assert!(place_hud_outside_focused_field(screen_rectangle(), 1300.0, 70.0, None).is_none());
    }

    #[test]
    fn hud_sits_above_the_caret_with_a_clear_gap() {
        let caret = ScreenRectangle {
            x: 550.0,
            y: 200.0,
            width: 1.0,
            height: 20.0,
        };
        let hud = place_hud_above_caret(screen_rectangle(), 400.0, 70.0, caret).unwrap();
        assert_eq!(hud.y, 236.0);
        assert!(!hud.intersects(caret.expanded_by(15.0)));
    }

    #[test]
    fn hud_uses_below_when_the_caret_is_at_the_top_edge() {
        let caret = ScreenRectangle {
            x: 550.0,
            y: 750.0,
            width: 1.0,
            height: 20.0,
        };
        let hud = place_hud_above_caret(screen_rectangle(), 400.0, 70.0, caret).unwrap();
        assert_eq!(hud.y, 664.0);
    }

    #[test]
    fn hud_near_caret_stays_inside_monitor_edges() {
        for x in [0.0, 1199.0] {
            let caret = ScreenRectangle {
                x,
                y: 200.0,
                width: 1.0,
                height: 20.0,
            };
            let hud = place_hud_above_caret(screen_rectangle(), 400.0, 70.0, caret).unwrap();
            assert!(hud.x >= 16.0 && hud.x + hud.width <= 1184.0);
        }
    }
}
