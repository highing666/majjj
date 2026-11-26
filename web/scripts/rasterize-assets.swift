import Foundation
import CoreGraphics
import CoreText
import ImageIO
import UniformTypeIdentifiers

// Deterministic @2x vector-to-PNG rasterizer for Style A ("玉骨工笔") dual-body tile atlas & impasto felt textures.

struct AlphaColorSpec: Decodable {
    let hex: String
    let alpha: Double
}

struct PaletteSpec: Decodable {
    let ink: String
    let inkSoft: AlphaColorSpec
    let crimson: String
    let cobalt: String
    let jade: String
    let ivory: String
    let ivoryTop: AlphaColorSpec
    let ivoryBottom: String
    let jadeBack: String
    let jadeBackDark: String
    let jadeBackHi: String
    let jadeBorder: String
    let jadeRim: AlphaColorSpec
    let goldDot: AlphaColorSpec
    let whiteBevel: AlphaColorSpec
    let bambooSpec: AlphaColorSpec
}

struct GridSpec: Decodable {
    let cols: Int
    let rows: Int
    let cellW: Int
    let cellH: Int
    let frameW: Int
    let frameH: Int
    let pad: Int
    let tileCount: Int
    let bodyFrames: [String]
}

struct BambooStalkSpec: Decodable {
    let x: Double
    let y: Double
    let h: Double
    let color: String
    let angle: Double
}

struct PlumPinSpec: Decodable {
    let x: Double
    let y: Double
    let r: Double
    let color: String
}

struct StyleASpec: Decodable {
    let grid: GridSpec
    let palette: PaletteSpec
    let bamboo: [String: [BambooStalkSpec]]
    let dot: [String: [PlumPinSpec]]
}

func cgColorFromHex(_ hex: String, alpha: Double = 1.0) -> CGColor {
    let clean = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
    let value = UInt64(clean, radix: 16) ?? 0
    let r = CGFloat((value >> 16) & 0xFF) / 255.0
    let g = CGFloat((value >> 8) & 0xFF) / 255.0
    let b = CGFloat(value & 0xFF) / 255.0
    return CGColor(srgbRed: r, green: g, blue: b, alpha: CGFloat(alpha))
}

struct Palette {
    let ink: CGColor
    let inkSoft: CGColor
    let crimson: CGColor
    let cobalt: CGColor
    let jade: CGColor
    let ivory: CGColor
    let ivoryShadow: CGColor
    let ivorySpec: CGColor
    let bambooSpec: CGColor
    let jadeBack: CGColor
    let jadeBackHi: CGColor
    let jadeBackDeep: CGColor
    let jadeBorder: CGColor
    let jadeRim: CGColor
    let goldDot: CGColor
    let whiteBevel: CGColor

    init(spec: PaletteSpec) {
        self.ink = cgColorFromHex(spec.ink)
        self.inkSoft = cgColorFromHex(spec.inkSoft.hex, alpha: spec.inkSoft.alpha)
        self.crimson = cgColorFromHex(spec.crimson)
        self.cobalt = cgColorFromHex(spec.cobalt)
        self.jade = cgColorFromHex(spec.jade)
        self.ivory = cgColorFromHex(spec.ivory)
        self.ivoryShadow = cgColorFromHex(spec.ivoryBottom)
        self.ivorySpec = cgColorFromHex(spec.ivoryTop.hex, alpha: spec.ivoryTop.alpha)
        self.bambooSpec = cgColorFromHex(spec.bambooSpec.hex, alpha: spec.bambooSpec.alpha)
        self.jadeBack = cgColorFromHex(spec.jadeBack)
        self.jadeBackHi = cgColorFromHex(spec.jadeBackHi)
        self.jadeBackDeep = cgColorFromHex(spec.jadeBackDark)
        self.jadeBorder = cgColorFromHex(spec.jadeBorder)
        self.jadeRim = cgColorFromHex(spec.jadeRim.hex, alpha: spec.jadeRim.alpha)
        self.goldDot = cgColorFromHex(spec.goldDot.hex, alpha: spec.goldDot.alpha)
        self.whiteBevel = cgColorFromHex(spec.whiteBevel.hex, alpha: spec.whiteBevel.alpha)
    }

    func resolve(_ key: String) -> CGColor {
        switch key {
        case "jade": return jade
        case "crimson": return crimson
        case "cobalt": return cobalt
        default: return ink
        }
    }
}

struct SeededRNG {
    private var state: UInt64
    init(seed: UInt64) { self.state = seed }
    mutating func next() -> Double {
        state = state &* 6364136223846793005 &+ 1442695040888963407
        let val = (state >> 32) & 0xFFFFFFFF
        return Double(val) / Double(UInt32.max)
    }
}

func makeContext(width: Int, height: Int) -> CGContext {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: cs,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.setAllowsAntialiasing(true)
    ctx.setShouldAntialias(true)
    ctx.translateBy(x: 0, y: CGFloat(height))
    ctx.scaleBy(x: 1, y: -1)
    return ctx
}

func savePNG(ctx: CGContext, url: URL) throws {
    guard let image = ctx.makeImage() else {
        throw NSError(domain: "MajjjAtlas", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to make CGImage"])
    }
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
        throw NSError(domain: "MajjjAtlas", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to create PNG destination"])
    }
    CGImageDestinationAddImage(dest, image, nil)
    if !CGImageDestinationFinalize(dest) {
        throw NSError(domain: "MajjjAtlas", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to finalize PNG"])
    }
}

func loadIllustrationForMultiply(url: URL) throws -> CGImage {
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
          let rawImage = CGImageSourceCreateImageAtIndex(src, 0, nil) else {
        throw NSError(domain: "MajjjAtlas", code: 4, userInfo: [NSLocalizedDescriptionKey: "Failed to load illustration \(url.path)"])
    }

    let w = rawImage.width
    let h = rawImage.height
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    var rgba = [UInt8](repeating: 0, count: w * h * 4)
    guard let ctx = CGContext(
        data: &rgba,
        width: w,
        height: h,
        bitsPerComponent: 8,
        bytesPerRow: w * 4,
        space: cs,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else {
        throw NSError(domain: "MajjjAtlas", code: 5, userInfo: [NSLocalizedDescriptionKey: "Failed to create bitmap for \(url.path)"])
    }
    ctx.draw(rawImage, in: CGRect(x: 0, y: 0, width: w, height: h))

    // Sample corner cream color and normalize white-point so warm-cream background becomes pure white
    // (1.0) under multiply blending without leaving a rectangular box against #F6EFE0/#FFFDF8.
    let samplePoints = [(8, 8), (w - 9, 8), (8, h - 9), (w - 9, h - 9)]
    var sumR = 0.0, sumG = 0.0, sumB = 0.0
    for (sx, sy) in samplePoints {
        let idx = (sy * w + sx) * 4
        sumR += Double(rgba[idx])
        sumG += Double(rgba[idx + 1])
        sumB += Double(rgba[idx + 2])
    }
    let bgR = max(180.0, (sumR / 4.0) - 4.0)
    let bgG = max(170.0, (sumG / 4.0) - 4.0)
    let bgB = max(150.0, (sumB / 4.0) - 4.0)

    var minX = w, maxX = -1, minY = h, maxY = -1
    for y in 0..<h {
        for x in 0..<w {
            let idx = (y * w + x) * 4
            let r = Double(rgba[idx])
            let g = Double(rgba[idx + 1])
            let b = Double(rgba[idx + 2])

            let dr = abs(r - bgR)
            let dg = abs(g - bgG)
            let db = abs(b - bgB)
            if dr + dg + db > 36.0 {
                if x < minX { minX = x }
                if x > maxX { maxX = x }
                if y < minY { minY = y }
                if y > maxY { maxY = y }
            }

            let nr = min(255.0, (r * 255.0) / bgR)
            let ng = min(255.0, (g * 255.0) / bgG)
            let nb = min(255.0, (b * 255.0) / bgB)
            rgba[idx] = UInt8(nr)
            rgba[idx + 1] = UInt8(ng)
            rgba[idx + 2] = UInt8(nb)
            rgba[idx + 3] = 255
        }
    }

    guard let normalized = ctx.makeImage() else {
        throw NSError(domain: "MajjjAtlas", code: 6, userInfo: [NSLocalizedDescriptionKey: "Failed to finalize normalized illustration"])
    }

    if minX <= maxX && minY <= maxY {
        let pad = 12
        let cropX = max(0, minX - pad)
        let cropY = max(0, minY - pad)
        let cropW = min(w - cropX, (maxX - minX + 1) + pad * 2)
        let cropH = min(h - cropY, (maxY - minY + 1) + pad * 2)
        if let cropped = normalized.cropping(to: CGRect(x: cropX, y: cropY, width: cropW, height: cropH)) {
            return cropped
        }
    }
    return normalized
}

func addRoundedRect(_ ctx: CGContext, rect: CGRect, radius: CGFloat) {
    let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    ctx.addPath(path)
}

func tileBodyRect(isLaid: Bool) -> CGRect {
    return CGRect(x: 2.5, y: isLaid ? 2.5 : 7.5, width: 95, height: isLaid ? 131 : 125)
}

func drawImpastoBody(_ ctx: CGContext, frame: CGRect, kind: String, palette: Palette) {
    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    let sx = frame.width / 100.0
    let sy = frame.height / 136.0
    ctx.scaleBy(x: sx, y: sy)

    if kind == "shadow" {
        let outer = CGRect(x: 4, y: 6, width: 92, height: 126)
        ctx.setFillColor(CGColor(srgbRed: 0.03, green: 0.05, blue: 0.07, alpha: 0.42))
        addRoundedRect(ctx, rect: outer, radius: 11)
        ctx.fillPath()
        let inner = CGRect(x: 6, y: 8, width: 88, height: 122)
        ctx.setFillColor(CGColor(srgbRed: 0.03, green: 0.05, blue: 0.07, alpha: 0.30))
        addRoundedRect(ctx, rect: inner, radius: 10)
        ctx.fillPath()
        ctx.restoreGState()
        return
    }

    let isBack = (kind == "back" || kind == "laid_back")
    let isLaid = (kind == "laid" || kind == "laid_back")

    // Standing tile deep jade spine thickness (visible at top & subtle bottom edge)
    if !isLaid {
        let spineRect = CGRect(x: 2.5, y: 1.5, width: 95, height: 133)
        ctx.saveGState()
        addRoundedRect(ctx, rect: spineRect, radius: 11)
        ctx.clip()
        let cs = CGColorSpace(name: CGColorSpace.sRGB)!
        if let grad = CGGradient(
            colorsSpace: cs,
            colors: [palette.jadeBack, palette.jadeBackDeep] as CFArray,
            locations: [0.0, 1.0]
        ) {
            ctx.drawLinearGradient(grad, start: CGPoint(x: 50, y: 1.5), end: CGPoint(x: 50, y: 134.5), options: [])
        } else {
            ctx.setFillColor(palette.jadeBack)
            ctx.fill(spineRect)
        }
        ctx.restoreGState()

        ctx.setStrokeColor(palette.jadeBorder)
        ctx.setLineWidth(1.8)
        addRoundedRect(ctx, rect: spineRect, radius: 11)
        ctx.strokePath()
    }

    let bodyRect = tileBodyRect(isLaid: isLaid)
    ctx.setFillColor(isBack ? palette.jadeBack : palette.ivory)
    addRoundedRect(ctx, rect: bodyRect, radius: 10.5)
    ctx.fillPath()

    ctx.saveGState()
    addRoundedRect(ctx, rect: bodyRect, radius: 10.5)
    ctx.clip()

    if isBack {
        let cs = CGColorSpace(name: CGColorSpace.sRGB)!
        if let grad = CGGradient(
            colorsSpace: cs,
            colors: [palette.jadeBackHi, palette.jadeBack, palette.jadeBackDeep] as CFArray,
            locations: [0.0, 0.58, 1.0]
        ) {
            ctx.drawLinearGradient(
                grad,
                start: CGPoint(x: bodyRect.minX, y: bodyRect.minY),
                end: CGPoint(x: bodyRect.maxX, y: bodyRect.maxY),
                options: []
            )
        }

        let inset = bodyRect.insetBy(dx: 10, dy: 10)
        ctx.setStrokeColor(palette.jadeRim)
        ctx.setLineWidth(2.2)
        addRoundedRect(ctx, rect: inset, radius: 7)
        ctx.strokePath()

        let cx: CGFloat = 50
        let cy = bodyRect.midY
        ctx.beginPath()
        ctx.move(to: CGPoint(x: cx, y: cy - 24))
        ctx.addLine(to: CGPoint(x: cx + 16, y: cy))
        ctx.addLine(to: CGPoint(x: cx, y: cy + 24))
        ctx.addLine(to: CGPoint(x: cx - 16, y: cy))
        ctx.closePath()
        ctx.setLineWidth(2.2)
        ctx.strokePath()

        ctx.setFillColor(palette.goldDot)
        ctx.fillEllipse(in: CGRect(x: cx - 5, y: cy - 5, width: 10, height: 10))
    } else {
        ctx.setFillColor(palette.ivorySpec)
        ctx.fill(CGRect(x: 4, y: bodyRect.minY + 0.8, width: 92, height: 5.5))

        ctx.setFillColor(palette.ivoryShadow)
        let lipH: CGFloat = isLaid ? 6.5 : 7.5
        ctx.fill(CGRect(x: 4, y: bodyRect.maxY - lipH - 0.5, width: 92, height: lipH))
    }
    ctx.restoreGState()

    ctx.setStrokeColor(isBack ? palette.jadeBorder : palette.ink)
    ctx.setLineWidth(2.6)
    addRoundedRect(ctx, rect: bodyRect, radius: 10.5)
    ctx.strokePath()

    ctx.restoreGState()
}

func drawCenteredText(_ ctx: CGContext, text: String, x: CGFloat, y: CGFloat, fontSize: CGFloat, fill: CGColor) {
    let fontCandidates = ["Kaiti SC", "STKaiti", "BiauKai", "Songti SC", "PingFang SC"]
    var chosenFont: CTFont?
    for name in fontCandidates {
        let f = CTFontCreateWithName(name as CFString, fontSize, nil)
        if CTFontCopyFamilyName(f) as String == name || name == "PingFang SC" {
            chosenFont = f
            break
        }
    }
    let font = chosenFont ?? CTFontCreateWithName("PingFang SC" as CFString, fontSize, nil)

    let attrs: [NSAttributedString.Key: Any] = [
        NSAttributedString.Key(kCTFontAttributeName as String): font,
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): fill
    ]

    let attrStr = NSAttributedString(string: text, attributes: attrs)
    let line = CTLineCreateWithAttributedString(attrStr)
    let bounds = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)

    ctx.saveGState()
    ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
    let drawX = x - bounds.midX
    let drawY = y + bounds.midY
    ctx.textPosition = CGPoint(x: drawX, y: drawY)
    CTLineDraw(line, ctx)
    ctx.restoreGState()
}

func drawPlumPin(_ ctx: CGContext, cx: CGFloat, cy: CGFloat, r: CGFloat, fill: CGColor, palette: Palette) {
    ctx.saveGState()
    let rect = CGRect(x: cx - r, y: cy - r, width: r * 2, height: r * 2)
    ctx.setFillColor(fill)
    ctx.fillEllipse(in: rect)
    ctx.setStrokeColor(palette.ink)
    ctx.setLineWidth(1.4)
    ctx.strokeEllipse(in: rect)

    let innerR = r * 0.74
    let innerRect = CGRect(x: cx - innerR, y: cy - innerR, width: innerR * 2, height: innerR * 2)
    ctx.setStrokeColor(palette.ivory)
    ctx.setAlpha(0.92)
    ctx.setLineWidth(max(1.1, r * 0.1))
    ctx.strokeEllipse(in: innerRect)
    ctx.setAlpha(1.0)

    let rx = r * 0.17
    let ry = r * 0.25
    let petalRect = CGRect(x: -rx, y: -r * 0.37 - ry, width: rx * 2, height: ry * 2)
    ctx.setFillColor(palette.ivory)
    for deg in [0.0, 72.0, 144.0, 216.0, 288.0] {
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.rotate(by: CGFloat(deg) * .pi / 180.0)
        ctx.fillEllipse(in: petalRect)
        ctx.restoreGState()
    }

    let hubR = r * 0.19
    ctx.setFillColor(fill)
    ctx.fillEllipse(in: CGRect(x: cx - hubR, y: cy - hubR, width: hubR * 2, height: hubR * 2))

    let holeR = r * 0.09
    ctx.setFillColor(palette.ivory)
    ctx.fillEllipse(in: CGRect(x: cx - holeR, y: cy - holeR, width: holeR * 2, height: holeR * 2))
    ctx.restoreGState()
}

func drawBambooStalk(_ ctx: CGContext, cx: CGFloat, cy: CGFloat, h: CGFloat, bw: CGFloat, color: CGColor, angleDeg: CGFloat = 0, palette: Palette) {
    ctx.saveGState()
    ctx.translateBy(x: cx, y: cy)
    ctx.rotate(by: angleDeg * .pi / 180.0)
    let top = -h * 0.5
    let bot = h * 0.5

    let stem = CGRect(x: -bw * 0.42, y: top + 2, width: bw * 0.84, height: h - 4)
    ctx.setFillColor(color)
    addRoundedRect(ctx, rect: stem, radius: 3.2)
    ctx.fillPath()
    ctx.setStrokeColor(palette.inkSoft)
    ctx.setLineWidth(1.1)
    addRoundedRect(ctx, rect: stem, radius: 3.2)
    ctx.strokePath()

    for ny in [top - 0.5, -2.3, bot - 4.1] {
        let node = CGRect(x: -bw * 0.58, y: ny, width: bw * 1.16, height: 4.6)
        ctx.setFillColor(color)
        addRoundedRect(ctx, rect: node, radius: 2.3)
        ctx.fillPath()
        ctx.setStrokeColor(palette.inkSoft)
        ctx.setLineWidth(1.1)
        addRoundedRect(ctx, rect: node, radius: 2.3)
        ctx.strokePath()
    }

    ctx.setStrokeColor(palette.bambooSpec)
    ctx.setLineWidth(1.4)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: -bw * 0.1, y: top + 5.5))
    ctx.addLine(to: CGPoint(x: -bw * 0.1, y: bot - 5.5))
    ctx.strokePath()
    ctx.restoreGState()
}

func drawNormalizedIllustration(_ ctx: CGContext, image: CGImage, targetRect: CGRect) {
    let imgW = CGFloat(image.width)
    let imgH = CGFloat(image.height)
    let scale = min(targetRect.width / imgW, targetRect.height / imgH)
    let drawW = imgW * scale
    let drawH = imgH * scale
    let drawRect = CGRect(
        x: targetRect.midX - drawW * 0.5,
        y: targetRect.midY - drawH * 0.5,
        width: drawW,
        height: drawH
    )

    ctx.saveGState()
    ctx.setBlendMode(.multiply)
    // Flip vertically to counteract top-left flipped CGContext when drawing CGImage
    ctx.translateBy(x: drawRect.minX, y: drawRect.maxY)
    ctx.scaleBy(x: 1, y: -1)
    ctx.draw(image, in: CGRect(x: 0, y: 0, width: drawW, height: drawH))
    ctx.restoreGState()
}

func drawTileFace(_ ctx: CGContext, frame: CGRect, tileIndex: Int, laid: Bool, spec: StyleASpec, palette: Palette, sou1Image: CGImage, pin1Image: CGImage) {
    drawImpastoBody(ctx, frame: frame, kind: laid ? "laid" : "standing", palette: palette)

    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    ctx.scaleBy(x: frame.width / 100.0, y: frame.height / 136.0)

    let bodyRect = tileBodyRect(isLaid: laid)
    addRoundedRect(ctx, rect: bodyRect, radius: 10.5)
    ctx.clip()

    let artScaleY: CGFloat = laid ? (128.0 / 130.0) : (121.0 / 130.0)
    let artOffsetY: CGFloat = laid ? 2.5 : 8.5
    ctx.translateBy(x: 0, y: artOffsetY)
    ctx.scaleBy(x: 1, y: artScaleY)

    let cnNums = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"]

    if tileIndex < 9 {
        let n = tileIndex + 1
        drawCenteredText(ctx, text: cnNums[n], x: 50, y: 39, fontSize: 42, fill: palette.ink)
        drawCenteredText(ctx, text: "萬", x: 50, y: 92, fontSize: 45, fill: palette.crimson)
    } else if tileIndex < 18 {
        let rank = tileIndex - 9 + 1
        let bw: CGFloat = rank >= 7 ? 10.2 : 12.2
        if rank == 1 {
            drawNormalizedIllustration(ctx, image: sou1Image, targetRect: CGRect(x: 7, y: 8, width: 86, height: 114))
        } else if let entries = spec.bamboo[String(rank)] {
            for entry in entries {
                drawBambooStalk(
                    ctx,
                    cx: CGFloat(entry.x),
                    cy: CGFloat(entry.y),
                    h: CGFloat(entry.h),
                    bw: bw,
                    color: palette.resolve(entry.color),
                    angleDeg: CGFloat(entry.angle),
                    palette: palette
                )
            }
        }
    } else if tileIndex < 27 {
        let rank = tileIndex - 18 + 1
        if rank == 1 {
            drawNormalizedIllustration(ctx, image: pin1Image, targetRect: CGRect(x: 6, y: 9, width: 88, height: 112))
        } else if let entries = spec.dot[String(rank)] {
            for entry in entries {
                drawPlumPin(
                    ctx,
                    cx: CGFloat(entry.x),
                    cy: CGFloat(entry.y),
                    r: CGFloat(entry.r),
                    fill: palette.resolve(entry.color),
                    palette: palette
                )
            }
        }
    } else if tileIndex == 33 {
        // Mainland plain white dragon (白板): faint inner recessed bevel only
        ctx.setStrokeColor(palette.whiteBevel)
        ctx.setLineWidth(1.6)
        addRoundedRect(ctx, rect: CGRect(x: 13, y: 14, width: 74, height: 102), radius: 7)
        ctx.strokePath()
    } else {
        let glyphs = [27: "東", 28: "南", 29: "西", 30: "北", 31: "中", 32: "發"]
        let color: CGColor = tileIndex == 31 ? palette.crimson : tileIndex == 32 ? palette.jade : palette.ink
        drawCenteredText(ctx, text: glyphs[tileIndex] ?? "?", x: 50, y: 65, fontSize: 63, fill: color)
    }

    ctx.restoreGState()
}

func drawToroidalStroke(_ ctx: CGContext, size: CGFloat, x: CGFloat, y: CGFloat, w: CGFloat, h: CGFloat, angle: CGFloat, color: CGColor, alpha: CGFloat) {
    ctx.saveGState()
    ctx.setAlpha(alpha)
    ctx.setFillColor(color)
    for dx in stride(from: -size, through: size, by: size) {
        for dy in stride(from: -size, through: size, by: size) {
            ctx.saveGState()
            ctx.translateBy(x: x + dx, y: y + dy)
            ctx.rotate(by: angle)
            addRoundedRect(ctx, rect: CGRect(x: -w * 0.5, y: -h * 0.5, width: w, height: h), radius: min(w, h) * 0.35)
            ctx.fillPath()
            ctx.restoreGState()
        }
    }
    ctx.restoreGState()
}

func buildFeltImpasto(url: URL) throws {
    let size = 512
    let fSize = CGFloat(size)
    let ctx = makeContext(width: size, height: size)

    ctx.setFillColor(CGColor(srgbRed: 0x14 / 255.0, green: 0x28 / 255.0, blue: 0x2E / 255.0, alpha: 1.0))
    ctx.fill(CGRect(x: 0, y: 0, width: fSize, height: fSize))

    var rng = SeededRNG(seed: 104729)
    let lightGold = CGColor(srgbRed: 0xF6 / 255.0, green: 0xD6 / 255.0, blue: 0x98 / 255.0, alpha: 1.0)
    let midTeal = CGColor(srgbRed: 0x2A / 255.0, green: 0x5C / 255.0, blue: 0x64 / 255.0, alpha: 1.0)
    let deepSlate = CGColor(srgbRed: 0x0A / 255.0, green: 0x16 / 255.0, blue: 0x1C / 255.0, alpha: 1.0)

    for _ in 0..<420 {
        let x = rng.next() * Double(size)
        let y = rng.next() * Double(size)
        let w = 24.0 + rng.next() * 68.0
        let h = 4.0 + rng.next() * 11.0
        let angle = -0.42 + (rng.next() - 0.5) * 0.45
        let roll = rng.next()
        let color = roll < 0.22 ? lightGold : (roll < 0.68 ? midTeal : deepSlate)
        let alpha = roll < 0.22 ? (0.04 + rng.next() * 0.05) : (0.10 + rng.next() * 0.14)
        drawToroidalStroke(ctx, size: fSize, x: x, y: y, w: w, h: h, angle: angle, color: color, alpha: alpha)
    }

    for _ in 0..<1800 {
        let x = rng.next() * Double(size)
        let y = rng.next() * Double(size)
        let w = 1.5 + rng.next() * 3.0
        let h = 1.0 + rng.next() * 2.0
        let isLight = rng.next() > 0.52
        let toothColor = isLight
            ? CGColor(srgbRed: 1.0, green: 0.97, blue: 0.91, alpha: 0.045)
            : CGColor(srgbRed: 0.02, green: 0.04, blue: 0.06, alpha: 0.075)
        drawToroidalStroke(ctx, size: fSize, x: x, y: y, w: w, h: h, angle: 0, color: toothColor, alpha: 1.0)
    }

    try savePNG(ctx: ctx, url: url)
}

func buildFeltBrush(url: URL) throws {
    let size = 256
    let fSize = CGFloat(size)
    let ctx = makeContext(width: size, height: size)
    ctx.clear(CGRect(x: 0, y: 0, width: fSize, height: fSize))

    var rng = SeededRNG(seed: 398217)
    let warmStroke = CGColor(srgbRed: 0.96, green: 0.89, blue: 0.72, alpha: 1.0)
    let coolStroke = CGColor(srgbRed: 0.06, green: 0.11, blue: 0.15, alpha: 1.0)

    for _ in 0..<220 {
        let x = rng.next() * Double(size)
        let y = rng.next() * Double(size)
        let w = 18.0 + rng.next() * 44.0
        let h = 3.0 + rng.next() * 7.5
        let angle = -0.38 + (rng.next() - 0.5) * 0.48
        let isLight = rng.next() > 0.48
        let alpha = isLight ? (0.06 + rng.next() * 0.08) : (0.08 + rng.next() * 0.10)
        drawToroidalStroke(ctx, size: fSize, x: x, y: y, w: w, h: h, angle: angle, color: isLight ? warmStroke : coolStroke, alpha: alpha)
    }

    try savePNG(ctx: ctx, url: url)
}

func main() throws {
    let args = CommandLine.arguments
    guard args.count >= 7 else {
        fputs("Usage: rasterize-assets.swift <atlas-png> <impasto-png> <brush-png> <spec-json> <1sou-png> <1pin-png>\n", stderr)
        exit(1)
    }

    let atlasUrl = URL(fileURLWithPath: args[1])
    let impastoUrl = URL(fileURLWithPath: args[2])
    let brushUrl = URL(fileURLWithPath: args[3])
    let specUrl = URL(fileURLWithPath: args[4])
    let sou1Url = URL(fileURLWithPath: args[5])
    let pin1Url = URL(fileURLWithPath: args[6])

    let specData = try Data(contentsOf: specUrl)
    let spec = try JSONDecoder().decode(StyleASpec.self, from: specData)
    let palette = Palette(spec: spec.palette)

    let sou1Image = try loadIllustrationForMultiply(url: sou1Url)
    let pin1Image = try loadIllustrationForMultiply(url: pin1Url)

    let cols = spec.grid.cols
    let rows = spec.grid.rows
    let cellW = spec.grid.cellW
    let cellH = spec.grid.cellH
    let atlasW = cols * cellW
    let atlasH = rows * cellH
    let frameW = spec.grid.frameW
    let frameH = spec.grid.frameH
    let pad = spec.grid.pad

    let atlasCtx = makeContext(width: atlasW, height: atlasH)
    atlasCtx.clear(CGRect(x: 0, y: 0, width: atlasW, height: atlasH))

    func frameFor(slot: Int) -> CGRect {
        let col = slot % cols
        let row = slot / cols
        return CGRect(x: col * cellW + pad, y: row * cellH + pad, width: frameW, height: frameH)
    }

    for t in 0..<spec.grid.tileCount {
        drawTileFace(atlasCtx, frame: frameFor(slot: t), tileIndex: t, laid: false, spec: spec, palette: palette, sou1Image: sou1Image, pin1Image: pin1Image)
        drawTileFace(atlasCtx, frame: frameFor(slot: spec.grid.tileCount + t), tileIndex: t, laid: true, spec: spec, palette: palette, sou1Image: sou1Image, pin1Image: pin1Image)
    }

    for (idx, frameKey) in spec.grid.bodyFrames.enumerated() {
        let kind = frameKey.hasPrefix("body_") ? String(frameKey.dropFirst("body_".count)) : frameKey
        drawImpastoBody(atlasCtx, frame: frameFor(slot: spec.grid.tileCount * 2 + idx), kind: kind, palette: palette)
    }

    try savePNG(ctx: atlasCtx, url: atlasUrl)
    try buildFeltImpasto(url: impastoUrl)
    try buildFeltBrush(url: brushUrl)
}

try main()
