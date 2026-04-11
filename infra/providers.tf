provider "kubernetes" {
  config_path = var.KUBE_CONFIG_PATH
}

provider "helm" {
  kubernetes = {
    config_path = var.KUBE_CONFIG_PATH
  }
}
