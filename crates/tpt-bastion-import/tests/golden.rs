// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Golden-file tests: the migration examples must convert to exactly the
//! checked-in output, so any change to the converter shows up in review.

use tpt_bastion_import::{convert, import_tfstate};

#[test]
fn migration_example_converts_to_the_golden_bcl() {
    let tf = include_str!("../../../examples/migration/main.tf");
    let golden = include_str!("../../../examples/migration/main.bcl");
    let converted = convert(tf).expect("the example converts");
    assert_eq!(converted.bcl, golden);
    // Every construct the example uses that is not converted is reported.
    assert_eq!(converted.notes.len(), 4, "{:#?}", converted.notes);
}

#[test]
fn migration_example_state_imports_single_instances_only() {
    let text = include_str!("../../../examples/migration/terraform.tfstate");
    let import = import_tfstate(text).expect("the example state imports");
    let ids: Vec<&str> = import.resources.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["aws_s3_bucket.assets"]);
    assert_eq!(import.notes.len(), 2);
}
