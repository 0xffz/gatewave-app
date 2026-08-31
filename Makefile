# Builds Gatewave.app for macOS from the release binary.
#
#   make app       → dist/Gatewave.app (release build, icon, Info.plist, ad-hoc signature)
#   make open      → build and launch it
#   make install   → copy into /Applications
#   make icns      → only regenerate the .icns from assets/icon.png
#   make clean     → remove dist/ and the generated icon
#
# Needs the Xcode command-line tools (sips, iconutil, codesign, plutil).

APP_NAME    := Gatewave
BIN         := gatewave
BUNDLE_ID   := com.gatewave.app
VERSION     := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
MIN_MACOS   := 11.0

CARGO       ?= cargo
DIST        := dist
APP         := $(DIST)/$(APP_NAME).app
CONTENTS    := $(APP)/Contents
RELEASE_BIN := target/release/$(BIN)
ICON_SRC    := assets/icon.png
ICONSET     := $(DIST)/$(APP_NAME).iconset
ICNS        := $(DIST)/$(APP_NAME).icns

.PHONY: app release icns open install clean

app: $(CONTENTS)/MacOS/$(BIN) $(CONTENTS)/Info.plist $(CONTENTS)/Resources/$(APP_NAME).icns
	codesign --force --deep --sign - "$(APP)"
	plutil -lint "$(CONTENTS)/Info.plist"
	@echo "→ $(APP) ($(VERSION))"

release:
	$(CARGO) build --release

# The release binary is always rebuilt through cargo, which is its own incremental build.
$(RELEASE_BIN): release

$(CONTENTS)/MacOS/$(BIN): $(RELEASE_BIN)
	mkdir -p "$(dir $@)"
	cp "$<" "$@"

$(CONTENTS)/Resources/$(APP_NAME).icns: $(ICNS)
	mkdir -p "$(dir $@)"
	cp "$<" "$@"

$(CONTENTS)/Info.plist: Makefile Cargo.toml
	mkdir -p "$(dir $@)"
	printf '%s\n' \
	  '<?xml version="1.0" encoding="UTF-8"?>' \
	  '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
	  '<plist version="1.0">' \
	  '<dict>' \
	  '  <key>CFBundleName</key><string>$(APP_NAME)</string>' \
	  '  <key>CFBundleDisplayName</key><string>$(APP_NAME)</string>' \
	  '  <key>CFBundleIdentifier</key><string>$(BUNDLE_ID)</string>' \
	  '  <key>CFBundleExecutable</key><string>$(BIN)</string>' \
	  '  <key>CFBundleIconFile</key><string>$(APP_NAME)</string>' \
	  '  <key>CFBundlePackageType</key><string>APPL</string>' \
	  '  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>' \
	  '  <key>CFBundleShortVersionString</key><string>$(VERSION)</string>' \
	  '  <key>CFBundleVersion</key><string>$(VERSION)</string>' \
	  '  <key>LSMinimumSystemVersion</key><string>$(MIN_MACOS)</string>' \
	  '  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>' \
	  '  <key>NSHighResolutionCapable</key><true/>' \
	  '  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>' \
	  '</dict>' \
	  '</plist>' > "$@"

# .icns from the 1024 px master: every size macOS asks for, plus the @2x variants.
icns: $(ICNS)

$(ICNS): $(ICON_SRC)
	rm -rf "$(ICONSET)"
	mkdir -p "$(ICONSET)"
	for size in 16 32 128 256 512; do \
	  sips -z $$size $$size "$(ICON_SRC)" --out "$(ICONSET)/icon_$${size}x$${size}.png" >/dev/null; \
	  double=$$((size * 2)); \
	  sips -z $$double $$double "$(ICON_SRC)" --out "$(ICONSET)/icon_$${size}x$${size}@2x.png" >/dev/null; \
	done
	iconutil -c icns "$(ICONSET)" -o "$@"
	rm -rf "$(ICONSET)"

open: app
	open "$(APP)"

install: app
	rm -rf "/Applications/$(APP_NAME).app"
	cp -R "$(APP)" /Applications/
	@echo "→ /Applications/$(APP_NAME).app"

clean:
	rm -rf "$(DIST)"
