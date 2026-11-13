# SPDX-License-Identifier: Apache-2.0 OR MIT
#
# A small Terraform configuration used as a migration golden file.
# `bastion import tf examples/migration` converts it to main.bcl and lists the
# constructs that are not converted (see the notes in the expected output).

terraform {
  required_version = ">= 1.5"
}

provider "aws" {
  region = "ap-southeast-2"
}

variable "environment" {
  type        = string
  default     = "dev"
  description = "Deployment environment"
}

resource "aws_s3_bucket" "assets" {
  bucket = "demo-assets"
  acl    = "private"

  versioning {
    enabled = true
  }

  tags = {
    environment = "dev"
    team        = "platform"
  }
}

resource "aws_s3_bucket" "logs" {
  count  = 2
  bucket = "demo-logs"
}

resource "aws_s3_bucket" "backup" {
  bucket = aws_s3_bucket.assets.bucket
  acl    = var.environment
}

output "assets_bucket" {
  value = aws_s3_bucket.assets.bucket
}
