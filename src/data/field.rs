use std::{collections::HashMap, iter::FromIterator, sync::Arc};

use arrow2::{
    array::Array,
    datatypes::{DataType, Field as ArrowField},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;

use crate::data::{
    field_type::{FieldType, IntoFieldType},
    ConfFloat64,
};

#[derive(Debug)]
pub struct Field {
    pub name: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub config: Option<FieldConfig>,

    pub(crate) values: Arc<dyn Array>,

    pub(crate) type_info: TypeInfo,
}

impl Field {
    #[must_use]
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub(crate) fn to_arrow_field(&self) -> Result<ArrowField, serde_json::Error> {
        let metadata = match (self.labels.as_ref(), self.config.as_ref()) {
            (None, None) => None,
            (Some(l), None) => Some(
                IntoIterator::into_iter([("labels".to_string(), serde_json::to_string(&l)?)])
                    .collect(),
            ),
            (None, Some(c)) => Some(
                IntoIterator::into_iter([("config".to_string(), serde_json::to_string(&c)?)])
                    .collect(),
            ),
            (Some(l), Some(c)) => Some(
                IntoIterator::into_iter([
                    ("labels".to_string(), serde_json::to_string(&l)?),
                    ("config".to_string(), serde_json::to_string(&c)?),
                ])
                .collect(),
            ),
        };
        Ok(ArrowField {
            name: self.name.clone().unwrap_or_default(),
            data_type: self.values.data_type().clone(),
            nullable: self.type_info.nullable.unwrap_or_default(),
            dict_id: 0,
            dict_is_ordered: false,
            metadata,
        })
    }

    pub fn set_values<T, U>(&mut self, values: T) -> Result<(), crate::data::error::Error>
    where
        T: IntoIterator<Item = U>,
        U: FieldType,
        U::Array: Array + FromIterator<Option<U>> + 'static,
    {
        if &U::ARROW_DATA_TYPE != self.values.data_type() {
            return Err(crate::data::error::Error::DataTypeMismatch);
        }
        self.values = Arc::new(values.into_iter().map(Some).collect::<U::Array>());
        self.type_info.nullable = Some(false);
        Ok(())
    }

    pub fn set_values_opt<T, U>(&mut self, values: T) -> Result<(), crate::data::error::Error>
    where
        T: IntoIterator<Item = Option<U>>,
        U: FieldType,
        U::Array: Array + FromIterator<Option<U>> + 'static,
    {
        if &U::ARROW_DATA_TYPE != self.values.data_type() {
            return Err(crate::data::error::Error::DataTypeMismatch);
        }
        self.values = Arc::new(values.into_iter().collect::<U::Array>());
        self.type_info.nullable = Some(true);
        Ok(())
    }
}

#[cfg(test)]
impl PartialEq for Field {
    // TODO - actually check values here
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.labels == other.labels && self.config == other.config
    }
}

// Traits for creating a `Field` from various array-like things:
// iterators of both values and optional values, and arrays themselves.
// These need to be separate traits because otherwise the impls would conflict,
// as e.g. `Array` implements `IntoIterator`.

pub trait IntoField {
    fn into_field(self) -> Field;
}

impl<T, U, V> IntoField for T
where
    T: IntoIterator<Item = U>,
    U: IntoFieldType<ElementType = V>,
    V: FieldType,
    V::Array: Array + FromIterator<Option<V>> + 'static,
{
    fn into_field(self) -> Field {
        Field {
            name: None,
            labels: None,
            config: None,
            type_info: TypeInfo {
                frame: V::TYPE_INFO_TYPE,
                nullable: Some(false),
            },
            values: Arc::new(
                self.into_iter()
                    .map(U::into_field_type)
                    .collect::<V::Array>(),
            ),
        }
    }
}

pub trait ArrayIntoField {
    fn into_field(self) -> Field;
}

impl<T> ArrayIntoField for T
where
    T: Array + 'static,
{
    fn into_field(self) -> Field {
        Field {
            name: None,
            labels: None,
            config: None,
            type_info: TypeInfo {
                frame: TypeInfoType::from_datatype(self.data_type()),
                nullable: Some(true),
            },
            values: Arc::new(self),
        }
    }
}

pub trait IntoOptField {
    fn into_opt_field(self) -> Field;
}

impl<'a, T, U, V> IntoOptField for T
where
    T: IntoIterator<Item = Option<U>>,
    U: IntoFieldType<ElementType = V>,
    V: FieldType,
    V::Array: Array + FromIterator<Option<V>> + 'static,
{
    fn into_opt_field(self) -> Field {
        Field {
            name: None,
            labels: None,
            config: None,
            type_info: TypeInfo {
                frame: V::TYPE_INFO_TYPE,
                nullable: Some(true),
            },
            values: Arc::new(
                self.into_iter()
                    .map(|x| x.and_then(U::into_field_type))
                    .collect::<V::Array>(),
            ),
        }
    }
}

// pub trait IntoTimestampField {
//     fn into_field(self) -> Field;
// }

// impl<T> IntoTimestampField for T
// where T: IntoIterator<Item = U>,
//     U

#[skip_serializing_none]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeInfo {
    pub(crate) frame: TypeInfoType,
    #[serde(default)]
    pub(crate) nullable: Option<bool>,
}

impl TypeInfo {
    #[must_use]
    pub const fn simple_type(&self) -> SimpleType {
        self.frame.simple_type()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeInfoType {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    String,
    Bool,
    #[serde(rename = "time.Time")]
    Time,
}

impl TypeInfoType {
    fn from_datatype(other: &DataType) -> Self {
        match other {
            DataType::Int8 => Self::Int8,
            DataType::Int16 => Self::Int16,
            DataType::Int32 => Self::Int32,
            DataType::Int64 => Self::Int64,
            DataType::UInt8 => Self::UInt8,
            DataType::UInt16 => Self::UInt16,
            DataType::UInt32 => Self::UInt32,
            DataType::UInt64 => Self::UInt64,
            DataType::Float32 => Self::Float32,
            DataType::Float64 => Self::Float64,
            DataType::Utf8 | DataType::LargeUtf8 => Self::String,
            DataType::Boolean => Self::Bool,
            DataType::Timestamp(..) => Self::Time,
            DataType::Null
            | DataType::Float16
            | DataType::Binary
            | DataType::FixedSizeBinary(..)
            | DataType::LargeBinary
            | DataType::List(..)
            | DataType::FixedSizeList(..)
            | DataType::LargeList(..)
            | DataType::Struct(..)
            | DataType::Union(..)
            | DataType::Dictionary(..)
            | DataType::Decimal(..)
            | DataType::Extension(..)
            | DataType::Date32
            | DataType::Date64
            | DataType::Time32(..)
            | DataType::Time64(..)
            | DataType::Duration(..)
            | DataType::Interval(..) => {
                panic!()
            }
        }
    }
}

impl TypeInfoType {
    #[must_use]
    pub(crate) const fn simple_type(&self) -> SimpleType {
        match self {
            Self::Int8
            | Self::Int16
            | Self::Int32
            | Self::Int64
            | Self::UInt8
            | Self::UInt16
            | Self::UInt32
            | Self::UInt64
            | Self::Float32
            | Self::Float64 => SimpleType::Number,
            Self::String => SimpleType::String,
            Self::Bool => SimpleType::Boolean,
            Self::Time => SimpleType::Time,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SimpleType {
    Number,
    Boolean,
    String,
    Time,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldConfig {
    #[serde(default)]
    pub display_name: Option<String>,

    #[serde(default, rename = "displayNameFromDS")]
    pub display_name_from_ds: Option<String>,

    #[serde(default)]
    pub path: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub filterable: Option<bool>,

    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub decimals: Option<u16>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<ValueMapping>,
    #[serde(default)]
    pub thresholds: Option<ThresholdsConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<DataLink>,

    #[serde(default)]
    pub no_value: Option<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum SpecialValueMatch {
    True,
    False,
    Null,
    NaN,
    #[serde(rename = "null+nan")]
    NullAndNaN,
    Empty,
}

#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "options", rename_all = "camelCase")]
pub enum ValueMapping {
    ValueMapper(HashMap<String, ValueMappingResult>),
    SpecialValueMapper {
        #[serde(rename = "match")]
        match_: SpecialValueMatch,
        result: ValueMappingResult,
    },
    RangeValueMapper {
        from: ConfFloat64,
        to: ConfFloat64,
        result: ValueMappingResult,
    },
}

#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueMappingResult {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub index: Option<isize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThresholdsConfig {
    pub mode: ThresholdsMode,
    pub steps: Vec<Threshold>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Threshold {
    pub value: Option<ConfFloat64>,
    pub color: Option<String>,
    pub state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThresholdsMode {
    Absolute,
    Percentage,
}

#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataLink {
    pub title: Option<String>,
    pub target_blank: Option<bool>,
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use arrow2::array::PrimitiveArray;

    use super::*;

    #[test]
    fn create_field_from_vec_primitive() {
        let field = vec![1u32, 2, 3].into_field().name("yhat".to_string());
        assert_eq!(field.name.unwrap(), "yhat");
    }

    #[test]
    fn create_field_from_vec_opt_primitive() {
        let field = vec![Some(1u32), None, None]
            .into_opt_field()
            .name("yhat".to_string());
        assert_eq!(field.name.unwrap(), "yhat");
    }

    #[test]
    fn create_field_from_slice_primitive() {
        let field = [1u32, 2, 3].into_field().name("yhat".to_string());
        assert_eq!(field.name.unwrap(), "yhat");
    }

    #[test]
    fn create_field_from_slice_opt_primitive() {
        let field = [Some(1u32), None, None]
            .into_opt_field()
            .name("yhat".to_string());
        assert_eq!(field.name.unwrap(), "yhat");
    }

    #[test]
    fn create_field_from_array_primitive() {
        let array = PrimitiveArray::<u32>::from_slice([1u32, 2, 3]);
        let field = array.into_field().name("yhat".to_string());
        assert_eq!(field.name.unwrap(), "yhat");
    }
}