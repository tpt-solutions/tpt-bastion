# SPDX-License-Identifier: Apache-2.0 OR MIT
# Example: author Bastion infrastructure in Python and render .bcl / drive the CLI.

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from bastion import BastionDocument, s, b, ref, lst, mp, to_bcl

doc = BastionDocument(
    config={"name": "hello-bastion", "region": "us-east-1"},
    variables=[{"name": "region", "typeName": "string", "default": s("us-east-1")}],
    resources=[
        {
            "typeName": "aws_s3_bucket",
            "name": "my_bucket",
            "attributes": {
                "bucket": s("hello-bastion-bucket"),
                "acl": s("private"),
                "versioning": b(True),
                "count": {"kind": "number", "value": 3},
                "tags": mp(environment=s("dev"), team=s("platform")),
                "lifecycle_rules": lst(mp(enabled=b(True), prefix=s("logs/"))),
            },
        },
        {
            "typeName": "aws_s3_bucket",
            "name": "replica",
            "attributes": {
                "bucket": s("hello-bastion-bucket-replica"),
                "acl": s("private"),
                "tags": mp(environment=s("dev")),
            },
        },
    ],
    outputs=[
        {"name": "bucket_name", "value": ref("my_bucket", "bucket")},
        {"name": "bucket_arn", "value": ref("my_bucket", "arn")},
    ],
)

print(to_bcl(doc))
