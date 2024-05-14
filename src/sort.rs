use liquid_core::Expression;
use liquid_core::Result;
use liquid_core::Runtime;
use liquid_core::{
    Display_filter, Filter, FilterParameters, FilterReflection, FromFilterParameters, ParseFilter,
};
use liquid_core::{Value, ValueView};
use std::cmp;
use crate::where_glob::{invalid_input, as_sequence};


fn nil_safe_compare(a: Value, b: Value) -> Option<cmp::Ordering> {
    if a.is_nil() && b.is_nil() {
        Some(cmp::Ordering::Equal)
    } else if a.is_nil() {
        Some(cmp::Ordering::Greater)
    } else if b.is_nil() {
        Some(cmp::Ordering::Less)
    } else {
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
