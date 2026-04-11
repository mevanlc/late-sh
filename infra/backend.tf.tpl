terraform {
  backend "s3" {
    bucket                      = "__TF_STATE_BUCKET__"
    key                         = "terraform.tfstate"
    region                      = "auto"
    skip_credentials_validation = true
    skip_metadata_api_check     = true
    skip_region_validation      = true
    skip_requesting_account_id  = true
    skip_s3_checksum            = true
    endpoints = {
      s3 = "__S3_ENDPOINT__"
    }
  }
}
