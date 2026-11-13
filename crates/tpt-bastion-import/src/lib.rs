// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Migrate existing Terraform / OpenTofu configuration and state to Bastion.
//!
//! - [`convert`] turns a `.tf` file into a Bastion `.bcl` document. Anything it
//!   cannot translate faithfully is reported with its line number, not copied.
//! - [`import_tfstate`] reads a `terraform.tfstate` (format version 4) as Bastion
//!   resources, reporting the resources it skips.
//!
//! The migration is conservative by design: a construct that would change what is
//! created (such as `count`) is never approximated. Review every note before
//! applying the result.

pub mod convert;
pub mod hcl;
pub mod state;

pub use convert::{convert, ConvertError, Converted, Note};
pub use state::{import_tfstate, StateImport, StateImportError};
