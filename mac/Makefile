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
GHOSTTY_BUILD_DIR := $(CURRENT_MAKEFILE_DIR)/build/ghostty
GHOSTTY_XCFRAMEWORK_PATH := $(GHOSTTY_BUILD_DIR)/Frameworks/GhosttyKit.xcframework
GHOSTTY_MACOS_FRAMEWORK_DIR := $(GHOSTTY_BUILD_DIR)/Frameworks/macos
GHOSTTY_MACOS_FRAMEWORK_PATH := $(GHOSTTY_MACOS_FRAMEWORK_DIR)/GhosttyKit.framework
GHOSTTY_RESOURCE_PATH := $(GHOSTTY_BUILD_DIR)/Resources/ghostty
GHOSTTY_TERMINFO_PATH := $(GHOSTTY_BUILD_DIR)/Resources/terminfo
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
	command -v xcodebuild >/dev/null
	command -v rsync >/dev/null
	if ! command -v zig >/dev/null 2>&1 && ! command -v mise >/dev/null 2>&1; then \
		echo "error: install zig or mise to build Ghostty"; \
		exit 1; \
	fi

build-ghostty-xcframework: $(GHOSTTY_BUILD_OUTPUTS) # Build and stage Ghostty artifacts under mac/build/ghostty

$(GHOSTTY_BUILD_OUTPUTS): | doctor submodules
	cd $(GHOSTTY_SUBMODULE_PATH)
	if command -v mise >/dev/null 2>&1; then \
		zig_cmd='mise exec -- zig'; \
	else \
		zig_cmd='zig'; \
	fi
	eval "$$zig_cmd build -Doptimize=ReleaseFast -Demit-xcframework=true -Dsentry=false"
	mkdir -p $(dir $(GHOSTTY_XCFRAMEWORK_PATH))
	rsync -a --delete $(GHOSTTY_SUBMODULE_PATH)/macos/GhosttyKit.xcframework/ $(GHOSTTY_XCFRAMEWORK_PATH)/
	framework_src="$$(find $(GHOSTTY_XCFRAMEWORK_PATH) -type d -path '*/GhosttyKit.framework' -print -quit)"
	if [ -z "$$framework_src" ]; then \
		echo "error: GhosttyKit.framework not found in $(GHOSTTY_XCFRAMEWORK_PATH)"; \
		exit 1; \
	fi
	mkdir -p $(GHOSTTY_MACOS_FRAMEWORK_DIR)
	rsync -a --delete "$$framework_src/" $(GHOSTTY_MACOS_FRAMEWORK_PATH)/
	mkdir -p $(GHOSTTY_RESOURCE_PATH)
	rsync -a --delete $(GHOSTTY_SUBMODULE_PATH)/zig-out/share/ghostty/ $(GHOSTTY_RESOURCE_PATH)/
	mkdir -p $(GHOSTTY_TERMINFO_PATH)
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
	rm -rf $(GHOSTTY_BUILD_DIR)

clean: clean-ghostty # Remove generated build outputs
	rm -rf $(CURRENT_MAKEFILE_DIR)/build
