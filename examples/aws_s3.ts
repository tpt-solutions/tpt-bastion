// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  str,
  bool,
  ref,
  map,
  toBcl,
  type BastionDocument,
} from "../bindings/typescript/src/index.ts";

const doc: BastionDocument = {
  config: { name: "hello-bastion", region: "us-east-1" },
  resources: [
    {
      typeName: "aws_s3_bucket",
      name: "my_bucket",
      attributes: {
        bucket: str("hello-bastion-bucket"),
        acl: str("private"),
        versioning: bool(true),
        tags: map({ environment: str("dev") }),
      },
    },
  ],
  outputs: [{ name: "bucket_name", value: ref("my_bucket", "bucket") }],
};

console.log(toBcl(doc));
