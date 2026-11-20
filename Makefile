.PHONY: mbuild mbuild-release clean test test-sdk

# Сборка RPM под Sailfish (Docker sailo-rs + mb2). Debug по умолчанию (быстро),
# для релиза — `make mbuild-release` (с оптимизациями).
mbuild:
	sg docker -c "./mbuild.sh"

mbuild-release:
	sg docker -c "./mbuild.sh --release"

# Тесты чистых крейтов (протокол, markdown) на хосте (без SFOS-тулчейна) — быстро.
test:
	cargo test --manifest-path opencode-proto/Cargo.toml
	cargo test --manifest-path opencode-markdown/Cargo.toml

# Тесты библиотеки opencode-client (линкует Qt/SFOS) — в SDK через sb2+qemu.
test-sdk:
	sg docker -c "./scripts/test-sdk.sh"

clean:
	rm -rf opencode-client/target opencode-proto/target opencode-markdown/target RPMS
