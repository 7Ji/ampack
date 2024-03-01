#[macro_export] macro_rules! impl_struct_try_from_ptr {
    ($stype: ident, $ptype: ident) => {
        impl TryFrom<*const $ptype> for $stype {
            type Error = Error;
            fn try_from(value: *const $ptype) -> Result<Self> {
                (&(unsafe {value.read()})).try_into()
            }
        }       
    };
}

#[macro_export] macro_rules! impl_struct_from_ptr {
    ($stype: ident, $ptype: ident) => {
        impl From<*const $ptype> for $stype {
            fn from(value: *const $ptype) -> Self {
                (&(unsafe {value.read()})).into()
            }
        }
    };
}