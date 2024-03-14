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
