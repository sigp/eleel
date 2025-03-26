variable "GITHUB_REPO" {
  default = "sigp/eleel"
}

variable "DESCRIPTION" {
  default = "Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus clients to connect to a single execution client."
}

group "default" {
  targets = ["binary-amd64", "binary-arm64"]
  
  context = "."
  
  

}

target "binary-amd64" {
  dockerfile = "Dockerfile.cross"
  context = "."
  platforms = ["linux/amd64"]
  description = "${DESCRIPTION}"
  tags = ["${GITHUB_REPO}-amd64"]
  labels = {
    "org.opencontainers.image.source" = "https://github.com/${GITHUB_REPO}"
  }

  args = {
    TARGET_ARCH = "x86_64-unknown-linux-gnu"
  }
}

target "binary-arm64" {
  dockerfile = "Dockerfile.cross"
  description = "${DESCRIPTION}"
  context = "."
  platforms = ["linux/arm64"]
  tags = ["${GITHUB_REPO}-arm64"]
  labels = {
    "org.opencontainers.image.source" = "https://github.com/${GITHUB_REPO}"
  }

  args = {
    TARGET_ARCH = "aarch64-unknown-linux-gnu"
  }
}

target "manifest" {
  inherits = ["binary"]
  output = ["type=registry"]
  description = "Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus clients to connect to a single execution client."
}
