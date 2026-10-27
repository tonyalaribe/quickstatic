use liquid_core::Expression;
use liquid_core::Result;
use liquid_core::Runtime;
use liquid_core::{
    Display_filter, Filter, FilterParameters, FilterReflection, FromFilterParameters, ParseFilter,
};
use liquid_core::{Value, ValueView};
use std::cmp;
use crate::where_glob::{invalid_input, as_sequence};
use chrono::{DateTime, NaiveDateTime, NaiveDate};

fn try_parse_date(s: &str) -> Option<i64> {
    // Try ISO 8601 datetime with timezone
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp());
    }

    // Try ISO 8601 datetime without timezone
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt.and_utc().timestamp());
    }

    // Try date only (YYYY-MM-DD)
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp());
    }

    // Try alternative formats (MM/DD/YYYY, DD-MM-YYYY, etc.)
    let formats = [
        "%Y/%m/%d",
        "%d/%m/%Y",
        "%m/%d/%Y",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
    ];

    for format in &formats {
        if let Ok(d) = NaiveDate::parse_from_str(s, format) {
            return Some(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp());
        }
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, format) {
            return Some(dt.and_utc().timestamp());
        }
    }

    None
}

// Values paired with their parsed date (if any), so dates are parsed once per element rather than per comparison.
type SortKey = (Option<i64>, Value);

fn sort_key(v: Value) -> SortKey {
    (v.as_scalar().and_then(|s| try_parse_date(s.to_kstr().as_str())), v)
}

fn nil_safe_compare((da, a): &SortKey, (db, b): &SortKey) -> Option<cmp::Ordering> {
    match (a.is_nil(), b.is_nil(), da, db) {
        (true, true, ..) => Some(cmp::Ordering::Equal),
        (true, ..) => Some(cmp::Ordering::Greater),
        (_, true, ..) => Some(cmp::Ordering::Less),
        (_, _, Some(x), Some(y)) => Some(x.cmp(y)),
        _ => a.partial_cmp(b),
    }
}

#[derive(Debug, Default, FilterParameters)]
struct PropertyArgs {
    #[parameter(description = "The property accessed by the filter.", arg_type = "str")]
    property: Option<Expression>,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "sort",
    description = "Sorts items in an array. The order of the sorted array is case-sensitive.",
    parameters(PropertyArgs),
    parsed(SortFilter)
)]
pub struct Sort;

#[derive(Debug, Default, FromFilterParameters, Display_filter)]
#[name = "sort"]
struct SortFilter {
    #[parameters]
    args: PropertyArgs,
}

fn property<'a>(value: &'a dyn ValueView, path: &str) -> Option<&'a dyn ValueView> {
    path.split('.').try_fold(value, |v, key| v.as_object()?.get(key))
}

impl Filter for SortFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let args = self.args.evaluate(runtime)?;

        let input: Vec<_> = as_sequence(input).collect();
        if args.property.is_some() && !input.iter().all(|v| v.is_object()) {
            return Err(invalid_input("Array of objects expected"));
        }

        let mut sorted: Vec<(SortKey, &dyn ValueView)> = input
            .into_iter()
            .map(|v| {
                let key = match &args.property {
                    Some(p) => property(v, p).map_or(Value::Nil, |x| x.to_value()),
                    None => v.to_value(),
                };
                (sort_key(key), v)
            })
            .collect();
        sorted.sort_by(|(a, _), (b, _)| nil_safe_compare(a, b).unwrap_or(cmp::Ordering::Equal));
        Ok(Value::array(sorted.into_iter().map(|(_, v)| v.to_value())))
    }
}
