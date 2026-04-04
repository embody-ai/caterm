# Sensible defaults
.ONESHELL:
SHELL := bash
.SHELLFLAGS := -e -u -o pipefail -c
.DELETE_ON_ERROR:
MAKEFLAGS += --warn-undefined-variables
MAKEFLAGS += --no-builtin-rules

# Derived values (DO NOT TOUCH).
CURRENT_MAKEFILE_PATH := $(abspath $(lastword $(MAKEFILE_LIST)))
CURRENT_MAKEFILE_DIR := $(patsubst %/,%,$(dir $(CURRENT_MAKEFILE_PATH)))
PROJECT_PATH := $(CURRENT_MAKEFILE_DIR)/Caterm/Caterm.xcodeproj
SCHEME := Caterm
CONFIGURATION ?= Debug
DERIVED_DATA_PATH ?= $(CURRENT_MAKEFILE_DIR)/build/DerivedData
XCODEBUILD_FLAGS ?=

GHOSTTY_SUBMODULE_PATH := $(CURRENT_MAKEFILE_DIR)/ThirdParty/ghostty
GHOSTTY_FRAMEWORKS_DIR := $(CURRENT_MAKEFILE_DIR)/Caterm/Frameworks
GHOSTTY_RESOURCES_DIR := $(CURRENT_MAKEFILE_DIR)/Resources
GHOSTTY_XCFRAMEWORK_PATH := $(GHOSTTY_FRAMEWORKS_DIR)/GhosttyKit.xcframework
GHOSTTY_MACOS_FRAMEWORK_DIR := $(GHOSTTY_FRAMEWORKS_DIR)/macos
GHOSTTY_MACOS_FRAMEWORK_PATH := $(GHOSTTY_MACOS_FRAMEWORK_DIR)/GhosttyKit.framework
GHOSTTY_RESOURCE_PATH := $(GHOSTTY_RESOURCES_DIR)/ghostty
GHOSTTY_TERMINFO_PATH := $(GHOSTTY_RESOURCES_DIR)/terminfo
GHOSTTY_BUILD_OUTPUTS := $(GHOSTTY_XCFRAMEWORK_PATH) $(GHOSTTY_MACOS_FRAMEWORK_PATH) $(GHOSTTY_RESOURCE_PATH) $(GHOSTTY_TERMINFO_PATH)

.DEFAULT_GOAL := help
.PHONY: help bootstrap submodules doctor build-ghostty-xcframework build-app build-all run-app test clean clean-ghostty open-project

help: # Display this help.
	@echo "Run make with one of the following targets:"
	@echo
	@grep -Eh '^[a-z-]+:.*#' $(CURRENT_MAKEFILE_PATH) | sed -E 's/^(.*:)(.*#+)(.*)/  \1 @@@ \3 /' | column -t -s "@@@"

bootstrap: submodules # Prepare local macOS build prerequisites

submodules: # Sync and initialize git submodules
	git -C $(CURRENT_MAKEFILE_DIR)/.. submodule sync --recursive
	git -C $(CURRENT_MAKEFILE_DIR)/.. submodule update --init --recursive

doctor: # Check required local tools
	@set -euo pipefail; \
	command -v xcodebuild >/dev/null; \
	command -v rsync >/dev/null; \
	if ! command -v zig >/dev/null 2>&1 && ! command -v mise >/dev/null 2>&1; then \
		echo "error: install zig or mise to build Ghostty"; \
		exit 1; \
	fi

build-ghostty-xcframework: $(GHOSTTY_BUILD_OUTPUTS) # Build and stage Ghostty artifacts under mac/Caterm/Frameworks and mac/Resources

$(GHOSTTY_BUILD_OUTPUTS): | doctor submodules
	@set -euo pipefail; \
	cd $(GHOSTTY_SUBMODULE_PATH); \
	if command -v mise >/dev/null 2>&1; then \
		mise exec -- zig build -Doptimize=ReleaseFast -Demit-xcframework=true -Dsentry=false; \
	else \
		zig build -Doptimize=ReleaseFast -Demit-xcframework=true -Dsentry=false; \
	fi; \
	mkdir -p $(dir $(GHOSTTY_XCFRAMEWORK_PATH)); \
	rsync -a --delete $(GHOSTTY_SUBMODULE_PATH)/macos/GhosttyKit.xcframework/ $(GHOSTTY_XCFRAMEWORK_PATH)/; \
	slice_dir="$(GHOSTTY_XCFRAMEWORK_PATH)/macos-arm64_x86_64"; \
	if [ ! -f "$$slice_dir/libghostty.a" ]; then \
		echo "error: macOS Ghostty static library not found in $$slice_dir"; \
		exit 1; \
	fi; \
	mkdir -p $(GHOSTTY_MACOS_FRAMEWORK_DIR); \
	rm -rf $(GHOSTTY_MACOS_FRAMEWORK_PATH); \
	mkdir -p $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/A/Headers; \
	mkdir -p $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/A/Modules; \
	mkdir -p $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/A/Resources; \
	cp "$$slice_dir/libghostty.a" $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/A/GhosttyKit; \
	rsync -a --delete "$$slice_dir/Headers/" $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/A/Headers/; \
	cp "$$slice_dir/Headers/module.modulemap" $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/A/Modules/module.modulemap; \
	ln -sfn A $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/Current; \
	ln -sfn Versions/Current/GhosttyKit $(GHOSTTY_MACOS_FRAMEWORK_PATH)/GhosttyKit; \
	ln -sfn Versions/Current/Headers $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Headers; \
	ln -sfn Versions/Current/Modules $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Modules; \
	ln -sfn Versions/Current/Resources $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Resources; \
	printf '%s\n' \
		'<?xml version="1.0" encoding="UTF-8"?>' \
		'<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
		'<plist version="1.0">' \
		'<dict>' \
		'  <key>CFBundleDevelopmentRegion</key>' \
		'  <string>en</string>' \
		'  <key>CFBundleExecutable</key>' \
		'  <string>GhosttyKit</string>' \
		'  <key>CFBundleIdentifier</key>' \
		'  <string>co.embody.caterm.GhosttyKit</string>' \
		'  <key>CFBundleInfoDictionaryVersion</key>' \
		'  <string>6.0</string>' \
		'  <key>CFBundleName</key>' \
		'  <string>GhosttyKit</string>' \
		'  <key>CFBundlePackageType</key>' \
		'  <string>FMWK</string>' \
		'  <key>CFBundleShortVersionString</key>' \
		'  <string>1.0</string>' \
		'  <key>CFBundleVersion</key>' \
		'  <string>1</string>' \
		'</dict>' \
		'</plist>' > $(GHOSTTY_MACOS_FRAMEWORK_PATH)/Versions/A/Resources/Info.plist; \
	mkdir -p $(GHOSTTY_RESOURCE_PATH); \
	rsync -a --delete $(GHOSTTY_SUBMODULE_PATH)/zig-out/share/ghostty/ $(GHOSTTY_RESOURCE_PATH)/; \
	mkdir -p $(GHOSTTY_TERMINFO_PATH); \
	rsync -a --delete $(GHOSTTY_SUBMODULE_PATH)/zig-out/share/terminfo/ $(GHOSTTY_TERMINFO_PATH)/

build-app: # Build the current macOS app via Xcode
	xcodebuild \
		-project $(PROJECT_PATH) \
		-scheme $(SCHEME) \
		-configuration $(CONFIGURATION) \
		-derivedDataPath $(DERIVED_DATA_PATH) \
		build \
		$(XCODEBUILD_FLAGS)

build-all: build-ghostty-xcframework build-app # Build Ghostty artifacts and the app

run-app: build-app # Build then launch the macOS app
	settings="$$(xcodebuild -project $(PROJECT_PATH) -scheme $(SCHEME) -configuration $(CONFIGURATION) -derivedDataPath $(DERIVED_DATA_PATH) -showBuildSettings 2>/dev/null)"
	build_dir="$$(printf '%s\n' "$$settings" | awk -F ' = ' '/BUILT_PRODUCTS_DIR = / { print $$2; exit }')"
	full_product_name="$$(printf '%s\n' "$$settings" | awk -F ' = ' '/FULL_PRODUCT_NAME = / { print $$2; exit }')"
	exec_name="$$(printf '%s\n' "$$settings" | awk -F ' = ' '/EXECUTABLE_NAME = / { print $$2; exit }')"
	if [ -z "$$build_dir" ] || [ -z "$$full_product_name" ] || [ -z "$$exec_name" ]; then \
		echo "error: failed to resolve built app path"; \
		exit 1; \
	fi
	exec "$$build_dir/$$full_product_name/Contents/MacOS/$$exec_name"

test: # Run macOS app tests
	xcodebuild \
		test \
		-project $(PROJECT_PATH) \
		-scheme $(SCHEME) \
		-destination 'platform=macOS' \
		-derivedDataPath $(DERIVED_DATA_PATH) \
		CODE_SIGNING_ALLOWED=NO \
		CODE_SIGNING_REQUIRED=NO \
		CODE_SIGN_IDENTITY="" \
		$(XCODEBUILD_FLAGS)

open-project: # Open the Xcode project
	open $(PROJECT_PATH)

clean-ghostty: # Remove staged Ghostty build artifacts
	rm -rf $(GHOSTTY_XCFRAMEWORK_PATH) $(GHOSTTY_MACOS_FRAMEWORK_DIR) $(GHOSTTY_RESOURCES_DIR)

clean: clean-ghostty # Remove generated build outputs
	rm -rf $(CURRENT_MAKEFILE_DIR)/build
