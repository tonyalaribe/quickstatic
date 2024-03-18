use liquid_core::Error;
use liquid_core::Expression;
use liquid_core::Result;
use liquid_core::Runtime;
use liquid_core::{
    Display_filter, Filter, FilterParameters, FilterReflection, FromFilterParameters, ParseFilter,
};
use liquid_core::{Value, ValueCow, ValueView};

#[derive(Debug, FilterParameters)]
struct WhereGlobArgs {
    #[parameter(description = "The property being matched", arg_type = "str")]
    property: Expression,
    #[parameter(
        description = "The value the property is matched with",
        arg_type = "any"
    )]
    target_value: Option<Expression>,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "where_glob",
    description = "Filter the elements of an array to those with a certain property value. \
                   By default the target is any truthy value.",
    parameters(WhereGlobArgs),
    parsed(WhereGlobFilter)
)]
pub struct WhereGlob;

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "where_glob"]
struct WhereGlobFilter {
    #[parameters]
    args: WhereGlobArgs,
}

fn as_sequence<'k>(input: &'k dyn ValueView) -> Box<dyn Iterator<Item = &'k dyn ValueView> + 'k> {
    if let Some(array) = input.as_array() {
        array.values()
    } else if input.is_nil() {
        Box::new(vec![].into_iter())
    } else {
        Box::new(std::iter::once(input))
    }
}

impl Filter for WhereGlobFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let args = self.args.evaluate(runtime)?;
        let property: &str = &args.property;
        let target_value: Option<ValueCow<'_>> = args.target_value;

        if let Some(array) = input.as_array() {
            if !array.values().all(|v| v.is_object()) {
                return Ok(Value::Nil);
            }
        } else if !input.is_object() {
            return Err(invalid_input(
                "Array of objects or a single object expected",
            ));
        }

        let input = as_sequence(input);
        let array: Vec<_> = match target_value {
            None => input
                .filter_map(|v| v.as_object())
                .filter(|object| {
                    object
                        .get(property)
                        .map(|v| v.query_state(liquid_core::model::State::Truthy))
                        .unwrap_or(false)
                })
                .map(|object| object.to_value())
                .collect(),
            Some(target_value) => input
                .filter_map(|v| v.as_object())
                .filter(|object| {
                    object
                        .get(property)
                        .map(|value| {
                            glob_match::glob_match(
                                target_value.to_kstr().as_str(),
                                &value.as_scalar().unwrap().to_kstr().to_string(),
                            )
                        })
                        .unwrap_or(false)
                })
                .map(|object| object.to_value())
                .collect(),
        };
        Ok(Value::array(array))
    }
}

pub(crate) fn invalid_input<S>(cause: S) -> Error
where
    S: Into<liquid_core::model::KString>,
{
    Error::with_msg("Invalid input").context("cause", cause)
}


// 
//
//
//

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "ternary",
    description = "Ternary Operator.",
    parameters(TernaryArgs),
    parsed(TernaryFilter)
)]
pub struct Ternary;



#[derive(Debug, FilterParameters)]
struct TernaryArgs {
    #[parameter(description = "Value if condition is true", arg_type = "any")]
    true_value: Expression,

    #[parameter(description = "Value if condition is false", arg_type = "any")]
    false_value: Expression,
}


#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "ternary"]
struct TernaryFilter {
    #[parameters]
    args: TernaryArgs,
}

impl Filter for TernaryFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value, liquid_core::Error> {
        // Convert the input to a Value
        let input_value = input.to_value();

        // Attempt to convert the Value into a boolean
        let condition = input_value.as_scalar().and_then(|s| s.to_bool()).unwrap_or(false);

        let true_value = self.args.true_value.evaluate(runtime)?.into_owned();
        let false_value = self.args.false_value.evaluate(runtime)?.into_owned();

        if condition {
            Ok(true_value)
        } else {
            Ok(false_value)
        }
    }
}


// StartsWith filter 
//

#[derive(Debug, Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "starts_with",
    description = "Checks if a string starts with a specified prefix.",
    parameters(StartsWithArgs),
    parsed(StartsWithFilter)
)]
pub struct StartsWith;

#[derive(Debug, FilterParameters)]
struct StartsWithArgs {
    #[parameter(description = "The prefix to check for", arg_type = "str")]
    prefix: Expression,
}

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "starts_with"]
struct StartsWithFilter {
    #[parameters]
    args: StartsWithArgs,
}

impl Filter for StartsWithFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value, liquid_core::Error> {
        // Attempt to convert the input to a scalar and then to a string
        let input_str = input.to_value().as_scalar().ok_or_else(|| liquid_core::Error::with_msg("Input is not a scalar value"))?.to_kstr().into_string();

        // Evaluate the prefix argument and attempt to convert it to a string
        let prefix = self.args.prefix.evaluate(runtime)?.into_owned().as_scalar().ok_or_else(|| liquid_core::Error::with_msg("Prefix is not a scalar value"))?.to_kstr().into_string();

        // Check if the input string starts with the prefix
        let result = input_str.starts_with(&prefix);

        // Return the result as a Value
        Ok(Value::scalar(result))
    }
}


// Equals Filter 
//

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "equals",
    description = "Checks if a string equals a specified value.",
    parameters(EqualsArgs),
    parsed(EqualsFilter)
)]
pub struct Equals;

#[derive(Debug, FilterParameters)]
struct EqualsArgs {
    #[parameter(description = "The value to compare against", arg_type = "any")]
    compare_value: Expression,
}

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "equals"]
pub struct EqualsFilter {
    #[parameters]
    args: EqualsArgs,
}


impl Filter for EqualsFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value, liquid_core::Error> {
        // Convert the input to a Value
        let input_value = input.to_value();

        // Evaluate the compare_value argument and convert it to a Value
        let compare_value = self.args.compare_value.evaluate(runtime)?.into_owned();

        // Check if the input value equals the compare_value
        let result = input_value == compare_value;

        // Return the result as a Value
        Ok(Value::scalar(result))
    }
}
