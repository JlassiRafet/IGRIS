//! Where everything goes. Pure geometry: the same window size always gives the same areas, so
//! nothing moves when a message appears or an overlay opens.
use ratatui::layout::Rect;

/// Width of the step rail. A window narrower than `RAIL_MIN_WIDTH` gets a one-row strip instead.
pub const RAIL_WIDTH: u16 = 22;
pub const RAIL_MIN_WIDTH: u16 = 100;
/// Height of the wordmark band when there is room for it.
pub const BAND_TALL: u16 = 4;
/// From this height up (and on a UTF-8 terminal) the wordmark is shown in full.
pub const TALL_HEIGHT: u16 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Areas {
    pub band: Rect,
    pub rule: Rect,
    pub rail: Option<Rect>,
    pub strip: Option<Rect>,
    pub panel: Rect,
    pub toast: Rect,
    pub next: Rect,
    pub footer: Rect,
}

/// `big_brand` is true when the four-row wordmark is drawn (tall window, UTF-8 terminal).
pub fn compute(frame: Rect, big_brand: bool) -> Areas {
    // One column of air on each side.
    let a = Rect::new(
        frame.x + 1,
        frame.y,
        frame.width.saturating_sub(2),
        frame.height,
    );
    let band_h = if big_brand { BAND_TALL } else { 1 };
    let band = Rect::new(a.x, a.y, a.width, band_h);
    let rule = Rect::new(a.x, a.y + band_h, a.width, 1);
    let bottom = a.y + a.height;
    let footer = Rect::new(a.x, bottom - 1, a.width, 1);
    let next = Rect::new(a.x, bottom - 2, a.width, 1);
    let toast = Rect::new(a.x, bottom - 3, a.width, 1);
    let top = rule.y + 1;
    let body = Rect::new(a.x, top, a.width, toast.y.saturating_sub(top));
    if frame.width >= RAIL_MIN_WIDTH {
        let rail = Rect::new(body.x, body.y, RAIL_WIDTH, body.height);
        let panel = Rect::new(
            body.x + RAIL_WIDTH,
            body.y,
            body.width - RAIL_WIDTH,
            body.height,
        );
        Areas {
            band,
            rule,
            rail: Some(rail),
            strip: None,
            panel,
            toast,
            next,
            footer,
        }
    } else {
        let strip = Rect::new(body.x, body.y, body.width, 1);
        let panel = Rect::new(
            body.x,
            body.y + 1,
            body.width,
            body.height.saturating_sub(1),
        );
        Areas {
            band,
            rule,
            rail: None,
            strip: Some(strip),
            panel,
            toast,
            next,
            footer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(a: &Areas) -> Vec<Rect> {
        let mut v = vec![a.band, a.rule, a.panel, a.toast, a.next, a.footer];
        v.extend(a.rail);
        v.extend(a.strip);
        v
    }

    #[test]
    fn nothing_overlaps_and_everything_is_inside_the_frame() {
        for (w, h) in [(80, 24), (99, 29), (100, 30), (140, 40), (200, 60)] {
            let frame = Rect::new(0, 0, w, h);
            let a = compute(frame, h >= TALL_HEIGHT);
            let rects = all(&a);
            for r in &rects {
                assert!(r.width > 0 && r.height > 0, "{w}x{h}: empty area {r:?}");
                assert!(
                    r.right() <= frame.right() && r.bottom() <= frame.bottom(),
                    "{w}x{h}: {r:?} leaves the frame"
                );
            }
            for (i, x) in rects.iter().enumerate() {
                for y in &rects[i + 1..] {
                    assert!(!x.intersects(*y), "{w}x{h}: {x:?} overlaps {y:?}");
                }
            }
        }
    }

    #[test]
    fn the_rail_needs_a_hundred_columns_and_the_tall_band_thirty_rows() {
        assert!(compute(Rect::new(0, 0, 100, 30), true).rail.is_some());
        let narrow = compute(Rect::new(0, 0, 99, 30), true);
        assert!(narrow.rail.is_none() && narrow.strip.is_some());
        assert_eq!(compute(Rect::new(0, 0, 100, 30), true).band.height, 4);
        assert_eq!(compute(Rect::new(0, 0, 100, 29), false).band.height, 1);
    }

    #[test]
    fn the_bottom_three_rows_are_always_toast_next_and_hints() {
        for (w, h) in [(80, 24), (120, 50)] {
            let a = compute(Rect::new(0, 0, w, h), h >= TALL_HEIGHT);
            assert_eq!((a.toast.y, a.next.y, a.footer.y), (h - 3, h - 2, h - 1));
            assert_eq!(
                a.panel.bottom(),
                a.toast.y,
                "the panel ends where the toast row starts"
            );
        }
    }

    #[test]
    fn a_window_of_exactly_80x24_leaves_a_usable_panel() {
        let a = compute(Rect::new(0, 0, 80, 24), false);
        assert_eq!((a.panel.width, a.panel.height), (78, 18));
    }
}
