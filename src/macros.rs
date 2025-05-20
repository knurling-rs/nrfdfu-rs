/// Creates an enum that can be converted from and to a primitive type.
///
/// `Enum::from_primitive` will return an `Option<Enum>` corresponding to the matching variant.
///
/// This was copied almost verbatim from [smoltcp] and has been adapted to provide just conversion.
///
/// [smoltcp]: https://github.com/m-labs/smoltcp/blob/cd893e6ab60f094d684b37be7bc013bf79f0459d/src/macros.rs
macro_rules! primitive_enum {
    (
        $( #[$enum_attr:meta] )*
        $v:vis enum $name:ident($ty:ty) {
            $(
              $( #[$variant_attr:meta] )*
              $variant:ident = $value:expr
            ),* $(,)?
        }
    ) => {
        $( #[$enum_attr] )*
        $v enum $name {
            $(
              $( #[$variant_attr] )*
              $variant,
            )*
        }

        impl $name {
            $v fn from_primitive(prim: $ty) -> Option<Self> {
                match prim {
                    $( $value => Some($name::$variant), )*
                    _ => None,
                }
            }
        }

        impl ::std::convert::From<$name> for $ty {
            fn from(value: $name) -> Self {
                match value {
                    $( $name::$variant => $value, )*
                }
            }
        }
    }
}
