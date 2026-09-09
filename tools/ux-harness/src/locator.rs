//! Strict, post-layout selectors. No coordinate fallback on missing or ambiguous state.

use serde::Deserialize;
use crate::proto::WidgetSnapshot;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub id: Option<String>,
    pub text: Option<String>,
    pub widget_type: Option<String>,
    pub window_index: Option<usize>,
    pub value: Option<String>,
    pub within: Option<Box<Selector>>,
}

impl Selector {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_none() && self.text.is_none() && self.widget_type.is_none() && self.value.is_none() {
            return Err("selector needs id, text, widget_type or value".into());
        }
        if let Some(parent) = &self.within { parent.validate()?; }
        Ok(())
    }
}

pub fn unique_match<'a>(widgets: &'a [WidgetSnapshot], selector: &Selector) -> Result<&'a WidgetSnapshot, String> {
    selector.validate()?;
    let region = selector.within.as_ref().map(|parent| unique_match(widgets, parent)).transpose()?;
    let matches: Vec<_> = widgets.iter().filter(|widget| {
        widget.visible && widget.width > 0 && widget.height > 0
            && selector.id.as_ref().is_none_or(|v| &widget.id == v)
            && selector.text.as_ref().is_none_or(|v| widget.text.as_ref() == Some(v))
            && selector.widget_type.as_ref().is_none_or(|v| &widget.widget_type == v)
            && selector.window_index.is_none_or(|v| widget.window_index == v)
            && selector.value.as_ref().is_none_or(|v| widget.value.as_ref() == Some(v))
            && region.is_none_or(|r| widget.window_index == r.window_index
                && widget.x >= r.x && widget.y >= r.y
                && widget.x as i128 + widget.width as i128 <= r.x as i128 + r.width as i128
                && widget.y as i128 + widget.height as i128 <= r.y as i128 + r.height as i128)
    }).collect();
    match matches.as_slice() {
        [widget] => Ok(widget),
        [] => Err(format!("no visible match for {selector:?}")),
        _ => Err(format!("ambiguous selector {selector:?}: {} matches", matches.len())),
    }
}

pub fn input_center(widgets: &[WidgetSnapshot], target: &WidgetSnapshot, windowed: bool) -> Result<(f64, f64), String> {
    let (mut x, mut y) = (target.x as f64 + target.width as f64 / 2.0, target.y as f64 + target.height as f64 / 2.0);
    if windowed {
        let windows: Vec<_> = widgets.iter().filter(|w| w.widget_type == "Window" && w.visible && w.window_index == target.window_index).collect();
        let [window] = windows.as_slice() else { return Err("windowed input requires exactly one matching Window geometry".into()); };
        x -= window.x as f64;
        y -= window.y as f64;
    }
    Ok((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget() -> WidgetSnapshot {
        WidgetSnapshot { id: "send".into(), text: Some("发送".into()), visible: true, enabled: true, width: 80, height: 40, ..Default::default() }
    }

    #[test]
    fn locator_requires_one_visible_match() {
        let selector: Selector = serde_json::from_str(r#"{"text":"发送"}"#).unwrap();
        assert!(unique_match(&[], &selector).is_err());
        assert!(unique_match(&[widget(), widget()], &selector).is_err());
        assert!(unique_match(&[widget()], &selector).is_ok());
        let mut hidden = widget(); hidden.visible = false;
        assert!(unique_match(&[hidden], &selector).is_err());
        let empty: Selector = serde_json::from_str("{}").unwrap();
        assert!(unique_match(&[widget()], &empty).is_err());
    }

    #[test]
    fn standalone_coordinates_are_relative_to_the_observed_window() {
        let window = WidgetSnapshot { widget_type: "Window".into(), visible: true, x: 1080, y: 457, ..Default::default() };
        let target = WidgetSnapshot { x: 1525, y: 741, width: 390, height: 44, ..widget() };
        assert_eq!(input_center(&[window.clone()], &target, true).unwrap(), (640.0, 306.0));
        assert_eq!(input_center(&[window], &target, false).unwrap(), (1720.0, 763.0));
        assert!(input_center(&[], &target, true).is_err());
    }

    #[test]
    fn region_selector_distinguishes_sidebar_from_recent_rooms() {
        let sidebar = WidgetSnapshot { id: "rooms_list".into(), width: 300, height: 800, ..widget() };
        let left = WidgetSnapshot { id: "room_name".into(), text: Some("project 2".into()), x: 20, y: 40, ..widget() };
        let right = WidgetSnapshot { x: 400, ..left.clone() };
        let widgets = [sidebar, left, right];
        let selector: Selector = serde_json::from_str(r#"{"text":"project 2","within":{"id":"rooms_list"}}"#).unwrap();
        assert_eq!(unique_match(&widgets, &selector).unwrap().x, 20);
    }

    proptest::proptest! {
        #[test]
        fn prop_locator_never_picks_an_ambiguous_widget(count in 0usize..20) {
            let selector: Selector = serde_json::from_str(r#"{"id":"send"}"#).unwrap();
            proptest::prop_assert_eq!(unique_match(&vec![widget(); count], &selector).is_ok(), count == 1);
        }

        #[test]
        fn prop_windowed_coordinates_are_translation_invariant(dx in -10000i64..10000, dy in -10000i64..10000) {
            let window = WidgetSnapshot { widget_type: "Window".into(), visible: true, x: dx, y: dy, ..Default::default() };
            let target = WidgetSnapshot { x: dx + 20, y: dy + 30, ..widget() };
            proptest::prop_assert_eq!(input_center(&[window], &target, true).unwrap(), (60.0, 50.0));
        }
    }
}
