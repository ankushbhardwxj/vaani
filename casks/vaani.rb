cask "vaani" do
  version "0.1.0"
  sha256 :no_check # Update with real SHA256 on release

  url "https://github.com/anthropics/vaani/releases/download/v#{version}/Vaani-v#{version}-macos.dmg"
  name "Vaani"
  desc "Voice to polished text, right at your cursor"
  homepage "https://github.com/anthropics/vaani"

  app "Vaani.app"

  # Remove macOS quarantine flag so the app runs without Gatekeeper warnings.
  # This is required because the app is ad-hoc signed (no Apple Developer ID).
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-rd", "com.apple.quarantine", "#{appdir}/Vaani.app"],
                   sudo: false
  end

  uninstall quit: "com.vaani.app"

  zap trash: [
    "~/.vaani",
  ]
end
