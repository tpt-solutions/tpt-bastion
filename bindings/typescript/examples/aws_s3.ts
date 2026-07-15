// SPDX-License-Identifier: Apache-2.0 OR MIT

import { str, bool, ref, list, map, toBcl, type BastionDocument } from "../src/index.ts";
import { writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const doc: BastionDocument = {
  config: { name: "hello-bastion", region: "us-east-1" },
  variables: [{ name: "region", typeName: "string", default: str("us-east-1") }],
  resources: [
    {
      typeName: "aws_s3_bucket",
      name: "my_bucket",
      attributes: {
        bucket: str("hello-bastion-bucket"),
        acl: str("private"),
        versioning: bool(true),
        count: { kind: "number", value: 3 },
        tags: map({ environment: str("dev"), team: str("platform") }),
        lifecycle_rules: list(map({ enabled: bool(true), prefix: str("logs/") })),
      },
    },
    {
      typeName: "aws_s3_bucket",
      name: "replica",
      attributes: {
        bucket: str("hello-bastion-bucket-replica"),
        acl: str("private"),
        tags: map({ environment: str("dev") }),
      },
    },
  ],
  outputs: [
    { name: "bucket_name", value: ref("my_bucket", "bucket") },
    { name: "bucket_arn", value: ref("my_bucket", "arn") },
  ],
};

const bcl = toBcl(doc);
writeFileSync(join(tmpdir(), "aws_s3.bcl"), bcl);
console.log(bcl);
