terraform {
  backend "s3" {
    bucket                      = "late-sh-r-tf-state"
    key                         = "terraform.tfstate"
    region                      = "auto"
    skip_credentials_validation = true
    skip_metadata_api_check     = true
    skip_region_validation      = true
    skip_requesting_account_id  = true
    skip_s3_checksum            = true
    endpoints = {
      s3 = "https://8ecfba101ed3834cf19fd86e68fc325b.r2.cloudflarestorage.com"
    }
  }
}
