// swift-tools-version: 6.4
import PackageDescription

let package = Package(
    name: "ShaderDiagnostics",
    platforms: [.iOS(.v17)],
    products: [.library(name: "ShaderDiagnostics", targets: ["ShaderDiagnostics"])],
    targets: [
        .target(
            name: "ShaderDiagnostics",
            path: "Sources/ShaderDiagnostics",
            publicHeadersPath: "include",
            cSettings: [.unsafeFlags([
                "-fobjc-arc", "-fblocks", "-Wall", "-Wextra", "-Werror",
                "-Wno-unused-parameter", "-Wno-deprecated-declarations"
            ])],
            linkerSettings: [
                .linkedFramework("Foundation"),
                .linkedFramework("UIKit"),
                .linkedFramework("CoreGraphics")
            ]
        )
    ]
)
