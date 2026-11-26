import Foundation
import CoreGraphics
import CoreText
import ImageIO
import UniformTypeIdentifiers

// Deterministic comic-style Piltover app icon generator for Majjj.app.
// Usage: swift build-icon.swift <output-iconset-dir>

struct Palette {
    static let ink = CGColor(srgbRed: 0x18 / 255.0, green: 0x1A / 255.0, blue: 0x20 / 255.0, alpha: 1.0)
    static let jadeDeep = CGColor(srgbRed: 0x10 / 255.0, green: 0x37 / 255.0, blue: 0x3A / 255.0, alpha: 1.0)
    static let jadeFelt = CGColor(srgbRed: 0x1E / 255.0, green: 0x5C / 255.0, blue: 0x5E / 255.0, alpha: 1.0)
    static let jadeRim = CGColor(srgbRed: 0x39 / 255.0, green: 0x92 / 255.0, blue: 0x94 / 255.0, alpha: 1.0)
    static let ivory = CGColor(srgbRed: 0xF5 / 255.0, green: 0xEE / 255.0, blue: 0xDF / 255.0, alpha: 1.0)
    static let ivoryShadow = CGColor(srgbRed: 0xE2 / 255.0, green: 0xD4 / 255.0, blue: 0xB7 / 255.0, alpha: 1.0)
    static let crimson = CGColor(srgbRed: 0xC7 / 255.0, green: 0x35 / 255.0, blue: 0x2B / 255.0, alpha: 1.0)
    static let gold = CGColor(srgbRed: 0xE0 / 255.0, green: 0xA4 / 255.0, blue: 0x38 / 255.0, alpha: 1.0)
    static let whiteSpec = CGColor(srgbRed: 1.0, green: 0.99, blue: 0.96, alpha: 0.8)
}

func makeContext(size: Int) -> CGContext {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(
        data: nil,
        width: size,
        height: size,
        bitsPerComponent: 8,
        bytesPerRow: size * 4,
        space: cs,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.setAllowsAntialiasing(true)
    ctx.setShouldAntialias(true)
    ctx.translateBy(x: 0, y: CGFloat(size))
    ctx.scaleBy(x: 1, y: -1)
    let scale = CGFloat(size) / 1024.0
    ctx.scaleBy(x: scale, y: scale)
    return ctx
}

func drawComicIcon(_ ctx: CGContext) {
    // 1. Rounded squircle felt badge (macOS icon margin ~100px on 1024 canvas)
    let bgRect = CGRect(x: 84, y: 84, width: 856, height: 856)
    let bgPath = CGPath(roundedRect: bgRect, cornerWidth: 186, cornerHeight: 186, transform: nil)

    // Drop shadow beneath badge
    ctx.saveGState()
    ctx.translateBy(x: 0, y: 18)
    ctx.setFillColor(CGColor(srgbRed: 0.04, green: 0.07, blue: 0.08, alpha: 0.45))
    ctx.addPath(bgPath)
    ctx.fillPath()
    ctx.restoreGState()

    // Base felt fill + comic halftone/rim highlight
    ctx.setFillColor(Palette.jadeFelt)
    ctx.addPath(bgPath)
    ctx.fillPath()

    ctx.saveGState()
    ctx.addPath(bgPath)
    ctx.clip()

    // Subtle diagonal comic ray highlight
    ctx.setFillColor(Palette.jadeRim)
    ctx.setAlpha(0.28)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 84, y: 84))
    ctx.addLine(to: CGPoint(x: 680, y: 84))
    ctx.addLine(to: CGPoint(x: 84, y: 680))
    ctx.closePath()
    ctx.fillPath()

    // Deeper corner vignette at bottom-right
    ctx.setFillColor(Palette.jadeDeep)
    ctx.setAlpha(0.45)
    ctx.fill(CGRect(x: 84, y: 720, width: 856, height: 220))
    ctx.restoreGState()

    // Inner brass accent frame
    let innerRim = bgRect.insetBy(dx: 22, dy: 22)
    ctx.setStrokeColor(Palette.gold)
    ctx.setLineWidth(10)
    ctx.addPath(CGPath(roundedRect: innerRim, cornerWidth: 164, cornerHeight: 164, transform: nil))
    ctx.strokePath()

    // Bold comic ink outer outline
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(20)
    ctx.addPath(bgPath)
    ctx.strokePath()

    // 2. Tilted 3D comic mahjong tile in center ("中" crimson dragon tile)
    ctx.saveGState()
    ctx.translateBy(x: 512, y: 518)
    ctx.rotate(by: -0.07)

    let tileW: CGFloat = 468
    let tileH: CGFloat = 620
    let tileX = -tileW / 2.0
    let tileY = -tileH / 2.0

    // Hard comic offset ink shadow
    let hardShadow = CGRect(x: tileX + 26, y: tileY + 34, width: tileW, height: tileH)
    ctx.setFillColor(CGColor(srgbRed: 0.05, green: 0.07, blue: 0.08, alpha: 0.52))
    ctx.addPath(CGPath(roundedRect: hardShadow, cornerWidth: 58, cornerHeight: 58, transform: nil))
    ctx.fillPath()

    // Jade 3D tile back layer
    let backRect = CGRect(x: tileX, y: tileY + 32, width: tileW, height: tileH)
    ctx.setFillColor(Palette.jadeRim)
    ctx.addPath(CGPath(roundedRect: backRect, cornerWidth: 56, cornerHeight: 56, transform: nil))
    ctx.fillPath()
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(18)
    ctx.addPath(CGPath(roundedRect: backRect, cornerWidth: 56, cornerHeight: 56, transform: nil))
    ctx.strokePath()

    // Ivory tile face
    let faceRect = CGRect(x: tileX, y: tileY - 12, width: tileW, height: tileH)
    ctx.setFillColor(Palette.ivory)
    ctx.addPath(CGPath(roundedRect: faceRect, cornerWidth: 56, cornerHeight: 56, transform: nil))
    ctx.fillPath()

    // Face bottom bevel + top specular highlight
    ctx.saveGState()
    ctx.addPath(CGPath(roundedRect: faceRect, cornerWidth: 56, cornerHeight: 56, transform: nil))
    ctx.clip()
    ctx.setFillColor(Palette.ivoryShadow)
    ctx.fill(CGRect(x: tileX, y: faceRect.maxY - 38, width: tileW, height: 38))
    ctx.setFillColor(Palette.whiteSpec)
    ctx.fill(CGRect(x: tileX + 24, y: faceRect.minY + 14, width: tileW - 48, height: 18))
    ctx.restoreGState()

    // Comic ink border around face
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(18)
    ctx.addPath(CGPath(roundedRect: faceRect, cornerWidth: 56, cornerHeight: 56, transform: nil))
    ctx.strokePath()

    // Draw bold crimson "中" glyph using CoreText (top-left flipped back for text)
    ctx.saveGState()
    ctx.translateBy(x: 0, y: -10)
    ctx.scaleBy(x: 1, y: -1)
    let fontCandidates = ["Songti SC", "STSong", "PingFang SC", "Hiragino Sans GB"]
    var chosenFont: CTFont?
    for name in fontCandidates {
        let f = CTFontCreateWithName(name as CFString, 340, nil)
        if CTFontCopyFamilyName(f) as String == name || name == "PingFang SC" {
            chosenFont = f
            break
        }
    }
    let font = chosenFont ?? CTFontCreateWithName("PingFang SC" as CFString, 340, nil)
    let attrs: [NSAttributedString.Key: Any] = [
        NSAttributedString.Key(kCTFontAttributeName as String): font,
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): Palette.crimson,
        NSAttributedString.Key(kCTStrokeColorAttributeName as String): Palette.ink,
        NSAttributedString.Key(kCTStrokeWidthAttributeName as String): -2.2
    ]
    let attrStr = NSAttributedString(string: "中", attributes: attrs)
    let line = CTLineCreateWithAttributedString(attrStr)
    let bounds = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)
    let tx = -bounds.midX
    let ty = -bounds.midY
    ctx.textPosition = CGPoint(x: tx, y: ty)
    CTLineDraw(line, ctx)
    ctx.restoreGState()

    // Comic sparkle accent at top-right corner of tile
    ctx.setFillColor(Palette.gold)
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(8)
    let sx: CGFloat = tileW * 0.34
    let sy: CGFloat = -tileH * 0.36
    ctx.beginPath()
    ctx.move(to: CGPoint(x: sx, y: sy - 34))
    ctx.addLine(to: CGPoint(x: sx + 11, y: sy - 11))
    ctx.addLine(to: CGPoint(x: sx + 34, y: sy))
    ctx.addLine(to: CGPoint(x: sx + 11, y: sy + 11))
    ctx.addLine(to: CGPoint(x: sx, y: sy + 34))
    ctx.addLine(to: CGPoint(x: sx - 11, y: sy + 11))
    ctx.addLine(to: CGPoint(x: sx - 34, y: sy))
    ctx.addLine(to: CGPoint(x: sx - 11, y: sy - 11))
    ctx.closePath()
    ctx.drawPath(using: .fillStroke)

    ctx.restoreGState()
}

func writePng(size: Int, to url: URL) throws {
    let ctx = makeContext(size: size)
    drawComicIcon(ctx)
    guard let img = ctx.makeImage() else {
        throw NSError(domain: "MajjjIcon", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to make CGImage"])
    }
    guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
        throw NSError(domain: "MajjjIcon", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to create PNG destination"])
    }
    CGImageDestinationAddImage(dest, img, nil)
    if !CGImageDestinationFinalize(dest) {
        throw NSError(domain: "MajjjIcon", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to finalize PNG"])
    }
}

guard CommandLine.arguments.count == 2 else {
    fputs("Usage: swift build-icon.swift <output-iconset-dir>\n", stderr)
    exit(1)
}

let iconsetURL = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: iconsetURL, withIntermediateDirectories: true)

let specs: [(String, Int)] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]

for (filename, px) in specs {
    try writePng(size: px, to: iconsetURL.appendingPathComponent(filename))
}
