variable "ARCH" {
  default = "amd64"
}

variable "GITHUB_REPO" {
    default = "https://github.com/sigp/eleel"
}

group "default" {
  targets = ["binary"]
  platforms = ["linux/amd64", "linux/arm64"]
  labels = {
    "org.opencontainers.image.source" = "{GITHUB_REPO}"
  }
  context = "."
  attest = [
    "type=provenance,mode=max",
    "type=sbom",
  ]
}

target "binary" {
  dockerfile = "Dockerfile.cross"
  context = "."
  
}

target "manifest" {
  inherits = ["binary"]
  # platforms = ["linux/${ARCH}"]
  output = ["type=registry"]
  description = "Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus clients to connect to a single execution client."
}
