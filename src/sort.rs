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

fn nil_safe_compare(a: Value, b: Value) -> Option<cmp::Ordering> {
    if a.is_nil() && b.is_nil() {
        Some(cmp::Ordering::Equal)
    } else if a.is_nil() {
        Some(cmp::Ordering::Greater)
    } else if b.is_nil() {
        Some(cmp::Ordering::Less)
    } else {
        // Try to parse as dates if both are strings
        if let (Some(a_scalar), Some(b_scalar)) = (a.as_scalar(), b.as_scalar()) {
            let a_str = a_scalar.to_kstr();
            let b_str = b_scalar.to_kstr();

            if let (Some(a_timestamp), Some(b_timestamp)) =
                (try_parse_date(a_str.as_str()), try_parse_date(b_str.as_str())) {
                return Some(a_timestamp.cmp(&b_timestamp));
            }
        }

        a.partial_cmp(&b)
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

fn safe_property_getter<'a>(value: &'a Value, property: &str) ->Value {
    let mut current_value = value.to_value();
    
    for key in property.split('.') {
        if let Some(obj) = current_value.as_object() {
            if let Some(next_value) = obj.get(key) {
                current_value = next_value.to_value() ;
            } else {
                return Value::Nil;
            }
        } else {
            return Value::Nil;
        }
    }
    
    current_value
}

impl Filter for SortFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let args = self.args.evaluate(runtime)?;

        let input: Vec<_> = as_sequence(input).collect();
        if args.property.is_some() && !input.iter().all(|v| v.is_object()) {
            return Err(invalid_input("Array of objects expected"));
        }

        let mut sorted: Vec<Value> = input.iter().map(|v| v.to_value()).collect();
        if let Some(property) = &args.property {
            // Using unwrap is ok since all of the elements are objects
            sorted.sort_by(|a, b| {
                nil_safe_compare(
                    safe_property_getter(a, property),
                    safe_property_getter(b, property),
                )
                .unwrap_or(cmp::Ordering::Equal)
            });
        } else {
            sorted.sort_by(|a, b| nil_safe_compare(a.clone(), b.clone()).unwrap_or(cmp::Ordering::Equal));
        }
        Ok(Value::array(sorted))
    }
}
