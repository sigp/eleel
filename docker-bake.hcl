group "default" {
  targets = ["binary"]
  context = "."
}

target "binary" {
  dockerfile = "Dockerfile.cross"
  context = "."
  description = "Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus clients to connect to a single execution client.

It is suitable for monitoring and analytics but should not be used to run validators and will cause you to miss block proposals if you attempt this.

Eleel is written in Rust and makes use of components from Lighthouse."
}

target "manifest" {
  inherits = ["binary"]
  platforms = ["linux/arm64", "linux/amd64"]
  output = ["type=registry"]
}
