use std::collections::LinkedList;
use std::sync::Arc;

use crate::data_source::DataSource;

type VirtualAddress = usize;

struct MapEntry {
    source: Arc<dyn DataSource>,
    offset: usize,
    span: usize,
    addr: usize,
}

/// An address space.
pub struct AddressSpace {
    name: String,
    mappings: LinkedList<MapEntry>, // see below for comments
}

// comments about storing mappings
// Most OS code uses doubly-linked lists to store sparse data structures like
// an address space's mappings.
// Using Rust's built-in LinkedLists is fine. See https://doc.rust-lang.org/std/collections/struct.LinkedList.html
// But if you really want to get the zen of Rust, this is a really good read, written by the original author
// of that very data structure: https://rust-unofficial.github.io/too-many-lists/

// So, feel free to come up with a different structure, either a classic Rust collection,
// from a crate (but remember it needs to be #no_std compatible), or even write your own.
// See this ticket from Riley: https://github.com/dylanmc/cs393_vm_api/issues/10

impl AddressSpace {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            mappings: LinkedList::new(),
        }
    }

    /// Add a mapping from a `DataSource` into this `AddressSpace`.
    ///
    /// # Errors
    /// If the desired mapping is invalid.
    ///
    /// # Panics
    /// todo
    pub fn add_mapping<D: DataSource>(
        &self,
        source: &D,
        offset: usize,
        span: usize,
    ) -> Result<VirtualAddress, &str> {
        todo!()
    }

    /// Add a mapping from `DataSource` into this `AddressSpace` starting at a specific address.
    ///
    /// # Errors
    /// If there is insufficient room subsequent to `start`.
    ///
    /// # Panics
    /// todo
    pub fn add_mapping_at<D: DataSource>(
        &self,
        source: &D,
        offset: usize,
        span: usize,
        start: VirtualAddress,
    ) -> Result<(), &str> {
        todo!()
    }

    /// Remove the mapping to `DataSource` that starts at the given address.
    ///
    /// # Errors
    /// If the mapping could not be removed.
    ///
    /// # Panics
    /// todo
    pub fn remove_mapping<D: DataSource>(
        &self,
        source: &D,
        start: VirtualAddress,
    ) -> Result<(), &str> {
        todo!()
    }

    /// Look up the `DataSource` and offset within that `DataSource` for a
    /// `VirtualAddress` / `AccessType` in this `AddressSpace`
    ///
    /// # Errors
    /// If this `VirtualAddress` does not have a valid mapping in &self,
    /// or if this `AccessType` is not permitted by the mapping
    ///
    /// # Panics
    /// todo
    pub fn get_source_for_addr<D: DataSource>(
        &self,
        addr: VirtualAddress,
        access_type: Flags,
    ) -> Result<(&D, usize), &str> {
        todo!();
    }
}

// Visibility boundary to ensure private internals, so our validation scheme works properly.
mod flags {
    /// Build flags for address space maps.
    ///
    /// You should prefer the `flags` macro for constant-time creation of flags; this type is only used
    /// for dynamic creation.
    ///
    /// We recommend using this builder type as follows:
    /// ```
    /// # use reedos_address_space::Flags;
    /// let flags = Flags::build()
    ///     .toggle_read()
    ///     .toggle_write();
    /// ```
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    #[allow(clippy::struct_excessive_bools)] // clippy is wrong: bools are more readable than enums
                                             // here because these directly correspond to yes/no
                                             // hardware flags
    pub struct FlagBuilder {
        // TODO: should there be some sanity checks that conflicting flags are never toggled? can we do
        // this at compile-time? (the second question is maybe hard)
        pub read: bool,
        pub write: bool,
        pub execute: bool,
        pub cow: bool,
        pub private: bool,
        pub shared: bool,
    }

    /// Create a toggler for a `FlagBuilder` field.
    macro_rules! flag_toggle {
        (
        $flag:ident,
        $toggle:ident,
        $setter:ident
    ) => {
            #[doc=concat!("Toggle the ", stringify!($flag), " flag.")]
            #[must_use]
            pub const fn $toggle(self) -> Self {
                Self {
                    $flag: !self.$flag,
                    ..self
                }
            }

            #[doc=concat!("Set the ", stringify!($flag), " flag.")]
            #[must_use]
            pub const fn $setter(self, to: bool) -> Self {
                Self { $flag: to, ..self }
            }
        };
    }

    impl FlagBuilder {
        /// Create a new `FlagBuilder` with all flags toggled off.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Validate that the `FlagBuilder` represents valid flags.
        ///
        /// # Panics
        /// If the `FlagBuilder` represents invalid flags.
        #[must_use]
        pub fn validate(self) -> Flags {
            assert!(!(self.private && self.shared));
            Flags {
                read: self.read,
                write: self.write,
                execute: self.execute,
                cow: self.cow,
                private: self.private,
                shared: self.shared,
            }
        }

        flag_toggle!(read, toggle_read, set_read);
        flag_toggle!(write, toggle_write, set_write);
        flag_toggle!(execute, toggle_execute, set_execute);
        flag_toggle!(cow, toggle_cow, set_cow);
        flag_toggle!(private, toggle_private, set_private);
        flag_toggle!(shared, toggle_shared, set_shared);

        #[must_use]
        /// Combine two `FlagBuilder`s by boolean or-ing each of their flags.
        ///
        /// This is, somewhat counter-intuitively, named `and`, so that the following code reads
        /// correctly:
        ///
        /// ```
        /// # use reedos_address_space::Flags;
        /// let read = Flags::read();
        /// let execute = Flags::execute();
        /// let new = read.and(execute);
        /// assert_eq!(new, Flags::build().toggle_read().toggle_execute());
        /// ```
        pub const fn and(self, other: Self) -> Self {
            let read = self.read || other.read;
            let write = self.write || other.write;
            let execute = self.execute || other.execute;
            let cow = self.cow || other.cow;
            let private = self.private || other.private;
            let shared = self.shared || other.shared;

            Self {
                read,
                write,
                execute,
                cow,
                private,
                shared,
            }
        }

        #[must_use]
        /// Turn off all flags in self that are on in other.
        ///
        /// You can think of this as `self &! other` on each field.
        ///
        /// ```
        /// # use reedos_address_space::Flags;
        /// let read_execute = Flags::read().toggle_execute();
        /// let execute = Flags::execute();
        /// let new = read_execute.but_not(execute);
        /// assert_eq!(new, Flags::build().toggle_read());
        /// ```
        pub const fn but_not(self, other: Self) -> Self {
            let read = self.read && !other.read;
            let write = self.write && !other.write;
            let execute = self.execute && !other.execute;
            let cow = self.cow && !other.cow;
            let private = self.private && !other.private;
            let shared = self.shared && !other.shared;

            Self {
                read,
                write,
                execute,
                cow,
                private,
                shared,
            }
        }
    }

    /// Create a constructor for a `Flags` object.
    macro_rules! flag_constructor {
        (
        $flag:ident
    ) => {
            #[doc=concat!("Turn on only the ", stringify!($flag), " flag.")]
            #[must_use]
            pub fn $flag() -> FlagBuilder {
                FlagBuilder {
                    $flag: true,
                    ..FlagBuilder::default()
                }
            }
        };
    }

    /// Access flags for virtual memory.
    ///
    /// There are two ways to create a `Flags`:
    ///
    /// 1. The `FlagBuilder` type, in particular `Flags::build`, which has public fields and allows dynamic creation of flags.
    /// 2. The `flags` macro.
    #[allow(clippy::struct_excessive_bools)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Flags {
        read: bool,
        write: bool,
        execute: bool,
        cow: bool,
        private: bool,
        shared: bool,
    }

    impl Flags {
        #[must_use]
        pub fn build() -> FlagBuilder {
            FlagBuilder::new()
        }

        /// Convert a `Flags` into a `FlagBuilder`, whose fields can be modified; it will need to be
        /// re-validated to get back a `Flags` object.
        #[must_use]
        pub const fn into_builder(self) -> FlagBuilder {
            FlagBuilder {
                read: self.read,
                write: self.write,
                execute: self.execute,
                cow: self.cow,
                private: self.private,
                shared: self.shared,
            }
        }

        flag_constructor!(read);
        flag_constructor!(write);
        flag_constructor!(execute);
        flag_constructor!(cow);
        flag_constructor!(private);
        flag_constructor!(shared);
    }

    /// Create a new `Flag`s object.
    ///
    /// ```
    /// # use reedos_address_space::{Flags, flags};
    /// assert_eq!(flags![read, write], Flags::build().toggle_read().toggle_write().validate());
    /// ```
    #[macro_export]
    macro_rules! flags [
    ($($flag:ident),*) => {
        $crate::address_space::FlagBuilder {
            $(
                $flag: true,
            )*
            ..$crate::address_space::FlagBuilder::new()
            }
            .validate()
        };
    ];
}

pub use crate::flags;
pub use flags::{FlagBuilder, Flags};
