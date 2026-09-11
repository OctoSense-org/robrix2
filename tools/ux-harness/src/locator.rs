//! Strict, post-layout selectors. No coordinate fallback on missing or ambiguous state.

use serde::Deserialize;
use crate::proto::WidgetSnapshot;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub path: Option<Vec<String>>,
    pub id: Option<String>,
    pub text: Option<String>,
    pub widget_type: Option<String>,
    pub window_index: Option<usize>,
    pub value: Option<String>,
    pub within: Option<Box<Selector>>,
}

impl Selector {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_none() && self.text.is_none() && self.widget_type.is_none() && self.value.is_none() && self.path.is_none() {
            return Err("selector needs id, text, widget_type or value".into());
        }
        if let Some(path) = &self.path {
            let encoded_len = path.iter().map(String::len).sum::<usize>() + path.len().saturating_sub(1);
            if self.within.is_some() || encoded_len > 4096 || !(2..=32).contains(&path.len()) || path.iter().any(|segment| {
                segment.is_empty() || segment.len() > 128 || segment == "-" || !segment.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
            }) {
                return Err("path needs 2..32 safe named segments and cannot be combined with within".into());
            }
        }
        if let Some(parent) = &self.within { parent.validate()?; }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRect {
    pub id: String,
    pub widget_type: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

pub fn parse_path_query_rects(rows: &[String]) -> Result<Vec<QueryRect>, String> {
    if rows.len() > 256 { return Err("path query returned more than 256 rows".into()); }
    rows.iter().map(|row| {
        let fields: Vec<_> = row.split_whitespace().collect();
        if fields.len() != 7 || fields[0].parse::<usize>().is_err() {
            return Err("path query returned an invalid native rectangle".into());
        }
        let number = |index: usize| fields[index].parse::<i64>().map_err(|_| "path query returned an invalid coordinate".to_string());
        let rect = QueryRect { id: fields[1].into(), widget_type: fields[2].into(), x: number(3)?, y: number(4)?, width: number(5)?, height: number(6)? };
        if rect.width <= 0 || rect.height <= 0 { return Err("path query returned a non-interactive rectangle".into()); }
        Ok(rect)
    }).collect()
}

pub fn unique_path_match<'a>(widgets: &'a [WidgetSnapshot], selector: &Selector, rows: &[QueryRect], windowed: bool) -> Result<&'a WidgetSnapshot, String> {
    selector.validate()?;
    let path = selector.path.as_ref().ok_or_else(|| "native path matching requires a path selector".to_string())?;
    let [row] = rows else {
        return if rows.is_empty() { Err(format!("no visible match for {selector:?}")) } else { Err(format!("ambiguous selector {selector:?}: {} native path matches", rows.len())) };
    };
    if row.id != path[path.len() - 1] {
        return Err("native path row does not identify the requested leaf".into());
    }
    let matches: Vec<_> = widgets.iter().filter(|widget| {
        if !widget.visible || widget.width <= 0 || widget.height <= 0 { return false; }
        let mut x = widget.x;
        let mut y = widget.y;
        if windowed {
            let windows: Vec<_> = widgets.iter().filter(|candidate| candidate.widget_type == "Window" && candidate.visible && candidate.window_index == widget.window_index).collect();
            let [window] = windows.as_slice() else { return false; };
            x -= window.x;
            y -= window.y;
        }
        widget.id == row.id && widget.widget_type == row.widget_type && x == row.x && y == row.y
            && widget.width == row.width && widget.height == row.height
            && selector.id.as_ref().is_none_or(|value| &widget.id == value)
            && selector.text.as_ref().is_none_or(|value| widget.text.as_ref() == Some(value))
            && selector.widget_type.as_ref().is_none_or(|value| &widget.widget_type == value)
            && selector.window_index.is_none_or(|value| widget.window_index == value)
            && selector.value.as_ref().is_none_or(|value| widget.value.as_ref() == Some(value))
    }).collect();
    match matches.as_slice() {
        [widget] => Ok(widget),
        [] => Err(format!("no visible match for {selector:?}")),
        _ => Err(format!("ambiguous selector {selector:?}: {} snapshot matches", matches.len())),
    }
}

pub fn unique_match<'a>(widgets: &'a [WidgetSnapshot], selector: &Selector) -> Result<&'a WidgetSnapshot, String> {
    selector.validate()?;
    if selector.path.is_some() {
        return Err("path selectors require a correlated native Studio query".into());
    }
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

    #[test]
    fn path_match_uses_native_ownership_and_window_translation() {
        let selector: Selector = serde_json::from_str(r#"{"path":["info_button","inner_button"],"id":"inner_button"}"#).unwrap();
        let window = WidgetSnapshot { widget_type: "Window".into(), visible: true, x: 1080, y: 457, ..Default::default() };
        let target = WidgetSnapshot { id: "inner_button".into(), widget_type: "Button".into(), visible: true, enabled: true, x: 1228, y: 529, width: 40, height: 40, ..Default::default() };
        let rows = parse_path_query_rects(&["220 inner_button Button 148 72 40 40".into()]).unwrap();
        assert_eq!(unique_path_match(&[window, target], &selector, &rows, true).unwrap().id, "inner_button");
    }

    #[test]
    fn path_match_preserves_native_ambiguity_and_rejects_legacy_or_invalid_rows() {
        let selector: Selector = serde_json::from_str(r#"{"path":["info_button","inner_button"]}"#).unwrap();
        let widget = WidgetSnapshot { id: "inner_button".into(), widget_type: "Button".into(), visible: true, width: 40, height: 40, ..Default::default() };
        let row = QueryRect { id: "inner_button".into(), widget_type: "Button".into(), x: 0, y: 0, width: 40, height: 40 };
        assert!(unique_path_match(&[widget], &selector, &[], false).unwrap_err().starts_with("no visible match"));
        assert!(unique_path_match(&[], &selector, &[row.clone(), row], false).unwrap_err().starts_with("ambiguous selector"));
        assert!(parse_path_query_rects(&["DB info_button DockTabs 0 0 40 40".into()]).is_err());
        assert!(serde_json::from_str::<Selector>(r#"{"path":["info_button","-"]}"#).unwrap().validate().is_err());
        assert!(serde_json::from_str::<Selector>(r#"{"path":["info_button","inner_button"],"within":{"id":"root"}}"#).unwrap().validate().is_err());
        let wrong_leaf = QueryRect { id: "unrelated".into(), widget_type: "Button".into(), x: 0, y: 0, width: 40, height: 40 };
        assert!(unique_path_match(&[], &selector, &[wrong_leaf], false).unwrap_err().contains("requested leaf"));
    }

    #[test]
    fn snapshot_only_locator_rejects_paths_at_every_selector_depth() {
        let unrelated = widget();
        let direct: Selector = serde_json::from_str(r#"{"path":["info_button","inner_button"]}"#).unwrap();
        assert!(unique_match(&[unrelated.clone()], &direct).unwrap_err().contains("native Studio query"));
        let nested: Selector = serde_json::from_str(r#"{"id":"send","within":{"path":["info_button","inner_button"]}}"#).unwrap();
        assert!(unique_match(&[unrelated], &nested).unwrap_err().contains("native Studio query"));
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
