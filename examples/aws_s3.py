# SPDX-License-Identifier: Apache-2.0 OR MIT

import os
import sys

sys.path.insert(
    0, os.path.join(os.path.dirname(__file__), "..", "bindings", "python")
)

from bastion import BastionDocument, s, b, ref, mp, to_bcl

doc = BastionDocument(
    config={"name": "hello-bastion", "region": "us-east-1"},
    resources=[
        {
            "typeName": "aws_s3_bucket",
            "name": "my_bucket",
            "attributes": {
                "bucket": s("hello-bastion-bucket"),
                "acl": s("private"),
                "versioning": b(True),
                "tags": mp(environment=s("dev")),
            },
        }
    ],
    outputs=[{"name": "bucket_name", "value": ref("my_bucket", "bucket")}],
)

print(to_bcl(doc))
