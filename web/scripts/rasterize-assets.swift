import Foundation
import CoreGraphics
import CoreText
import ImageIO
import UniformTypeIdentifiers

// Deterministic @2x CoreGraphics/CoreText rasterizer for ADR-0013 Variant A ("国标机麻·厚玉雕版")
// dual-body physical mahjong tile atlas (10x8 grid, 192x256 cells, 184x248 frames, 1920x2048)
// and seamless deep-teal/warm-amber impasto felt textures.

struct AlphaColorSpec: Decodable {
    let hex: String
    let alpha: Double
}

struct GeometrySpec: Decodable {
    let cornerRadius: Double
    let standingTopCap: Double
    let laidIvoryWall: Double
    let laidJadeLip: Double
}

struct TypographySpec: Decodable {
    let primaryFont: String
    let fallbackFont: String
    let strokeWidthPct: Double
    let manNumeralSize: Double
    let manWanSize: Double
    let honorSize: Double
}

struct PaletteSpec: Decodable {
    let ink: String
    let crimson: String
    let jade: String
    let cobalt: String
    let goldAccent: String
    let ivoryTop: String
    let ivoryMid: String
    let ivoryBot: String
    let ivoryWallTop: String
    let ivoryWallBot: String
    let ivoryRimStroke: AlphaColorSpec
    let ivoryBevelLight: AlphaColorSpec
    let ivoryBevelShadow: AlphaColorSpec
    let jadeTopHi: String
    let jadeMid: String
    let jadeDeep: String
    let jadeRimStroke: AlphaColorSpec
    let jadeInlay: AlphaColorSpec
    let jadeGoldDot: AlphaColorSpec
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
    let geometry: GeometrySpec
    let typography: TypographySpec
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

func rgba(_ hex: UInt32, _ alpha: CGFloat = 1.0) -> CGColor {
    let r = CGFloat((hex >> 16) & 0xFF) / 255.0
    let g = CGFloat((hex >> 8) & 0xFF) / 255.0
    let b = CGFloat(hex & 0xFF) / 255.0
    return CGColor(srgbRed: r, green: g, blue: b, alpha: alpha)
}

struct TileTheme {
    let fontPS: String
    let fallbackFontPS: String
    let strokeWidthPct: CGFloat
    let manNumeralSize: CGFloat
    let manWanSize: CGFloat
    let honorSize: CGFloat

    let cornerRadius: CGFloat
    let standingTopCap: CGFloat
    let laidIvoryWall: CGFloat
    let laidJadeLip: CGFloat

    let ivoryTop: CGColor
    let ivoryMid: CGColor
    let ivoryBot: CGColor
    let ivoryWallTop: CGColor
    let ivoryWallBot: CGColor
    let ivoryRimStroke: CGColor
    let ivoryBevelLight: CGColor
    let ivoryBevelShadow: CGColor

    let jadeTopHi: CGColor
    let jadeMid: CGColor
    let jadeDeep: CGColor
    let jadeRimStroke: CGColor
    let jadeInlay: CGColor
    let jadeGoldDot: CGColor

    let ink: CGColor
    let crimson: CGColor
    let jade: CGColor
    let cobalt: CGColor
    let goldAccent: CGColor

    init(spec: StyleASpec) {
        self.fontPS = spec.typography.primaryFont
        self.fallbackFontPS = spec.typography.fallbackFont
        self.strokeWidthPct = CGFloat(spec.typography.strokeWidthPct)
        self.manNumeralSize = CGFloat(spec.typography.manNumeralSize)
        self.manWanSize = CGFloat(spec.typography.manWanSize)
        self.honorSize = CGFloat(spec.typography.honorSize)

        self.cornerRadius = CGFloat(spec.geometry.cornerRadius)
        self.standingTopCap = CGFloat(spec.geometry.standingTopCap)
        self.laidIvoryWall = CGFloat(spec.geometry.laidIvoryWall)
        self.laidJadeLip = CGFloat(spec.geometry.laidJadeLip)

        let p = spec.palette
        self.ivoryTop = cgColorFromHex(p.ivoryTop)
        self.ivoryMid = cgColorFromHex(p.ivoryMid)
        self.ivoryBot = cgColorFromHex(p.ivoryBot)
        self.ivoryWallTop = cgColorFromHex(p.ivoryWallTop)
        self.ivoryWallBot = cgColorFromHex(p.ivoryWallBot)
        self.ivoryRimStroke = cgColorFromHex(p.ivoryRimStroke.hex, alpha: p.ivoryRimStroke.alpha)
        self.ivoryBevelLight = cgColorFromHex(p.ivoryBevelLight.hex, alpha: p.ivoryBevelLight.alpha)
        self.ivoryBevelShadow = cgColorFromHex(p.ivoryBevelShadow.hex, alpha: p.ivoryBevelShadow.alpha)

        self.jadeTopHi = cgColorFromHex(p.jadeTopHi)
        self.jadeMid = cgColorFromHex(p.jadeMid)
        self.jadeDeep = cgColorFromHex(p.jadeDeep)
        self.jadeRimStroke = cgColorFromHex(p.jadeRimStroke.hex, alpha: p.jadeRimStroke.alpha)
        self.jadeInlay = cgColorFromHex(p.jadeInlay.hex, alpha: p.jadeInlay.alpha)
        self.jadeGoldDot = cgColorFromHex(p.jadeGoldDot.hex, alpha: p.jadeGoldDot.alpha)

        self.ink = cgColorFromHex(p.ink)
        self.crimson = cgColorFromHex(p.crimson)
        self.jade = cgColorFromHex(p.jade)
        self.cobalt = cgColorFromHex(p.cobalt)
        self.goldAccent = cgColorFromHex(p.goldAccent)
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
    ctx.interpolationQuality = .high
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

func addRoundedRect(_ ctx: CGContext, rect: CGRect, radius: CGFloat) {
    let r = min(radius, min(rect.width, rect.height) * 0.5)
    let path = CGPath(roundedRect: rect, cornerWidth: r, cornerHeight: r, transform: nil)
    ctx.addPath(path)
}

func drawLinearGrad(_ ctx: CGContext, colors: [CGColor], locations: [CGFloat], start: CGPoint, end: CGPoint) {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    if let grad = CGGradient(colorsSpace: cs, colors: colors as CFArray, locations: locations) {
        ctx.drawLinearGradient(grad, start: start, end: end, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
    }
}

func drawRadialGrad(_ ctx: CGContext, colors: [CGColor], locations: [CGFloat], center: CGPoint, startR: CGFloat, endR: CGFloat) {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    if let grad = CGGradient(colorsSpace: cs, colors: colors as CFArray, locations: locations) {
        ctx.drawRadialGradient(grad, startCenter: center, startRadius: startR, endCenter: center, endRadius: endR, options: [.drawsAfterEndLocation])
    }
}

func faceRectFor(laid: Bool, theme: TileTheme) -> CGRect {
    if laid {
        let faceH = 133.5 - 2.0 - theme.laidJadeLip - theme.laidIvoryWall
        return CGRect(x: 2.0, y: 2.0, width: 96.0, height: faceH)
    }
    let y = 1.5 + theme.standingTopCap
    let faceH = 133.5 - y
    return CGRect(x: 2.0, y: y, width: 96.0, height: faceH)
}

func drawPhysicalBody(_ ctx: CGContext, frame: CGRect, kind: String, theme: TileTheme) {
    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    ctx.scaleBy(x: frame.width / 100.0, y: frame.height / 135.0)

    let r = theme.cornerRadius

    if kind == "shadow" {
        let layers: [(CGRect, CGFloat, CGFloat)] = [
            (CGRect(x: 1.5, y: 3.5, width: 97, height: 130), r + 2.0, 0.14),
            (CGRect(x: 3.0, y: 5.5, width: 94, height: 127), r + 1.0, 0.22),
            (CGRect(x: 4.5, y: 7.5, width: 91, height: 124), r, 0.28),
            (CGRect(x: 6.5, y: 9.5, width: 87, height: 120), r - 1.0, 0.22),
        ]
        for (rect, rad, alpha) in layers {
            ctx.setFillColor(rgba(0x04090D, alpha))
            addRoundedRect(ctx, rect: rect, radius: rad)
            ctx.fillPath()
        }
        ctx.restoreGState()
        return
    }

    let isBack = (kind == "back" || kind == "laid_back")
    let isLaid = (kind == "laid" || kind == "laid_back")

    if !isLaid {
        // Standing tile: thick emerald jade back block extending above ivory face
        let backBlock = CGRect(x: 2.0, y: 1.5, width: 96.0, height: 132.0)
        ctx.saveGState()
        addRoundedRect(ctx, rect: backBlock, radius: r)
        ctx.clip()
        drawLinearGrad(
            ctx,
            colors: [theme.jadeTopHi, theme.jadeMid, theme.jadeDeep],
            locations: [0.0, 0.18, 1.0],
            start: CGPoint(x: 50, y: 1.5),
            end: CGPoint(x: 50, y: 133.5)
        )
        ctx.setFillColor(rgba(0xD8FFF0, 0.26))
        ctx.fill(CGRect(x: 6, y: 2.2, width: 88, height: 3.0))
        ctx.restoreGState()

        if isBack {
            let faceRect = faceRectFor(laid: false, theme: theme)
            ctx.saveGState()
            addRoundedRect(ctx, rect: faceRect, radius: r)
            ctx.clip()
            drawLinearGrad(
                ctx,
                colors: [theme.jadeTopHi, theme.jadeMid, theme.jadeDeep],
                locations: [0.0, 0.45, 1.0],
                start: CGPoint(x: 15, y: faceRect.minY),
                end: CGPoint(x: 85, y: faceRect.maxY)
            )
            ctx.setStrokeColor(rgba(0xD0FFEA, 0.30))
            ctx.setLineWidth(1.4)
            addRoundedRect(ctx, rect: faceRect.insetBy(dx: 1.0, dy: 1.0), radius: r - 0.8)
            ctx.strokePath()

            let inset = faceRect.insetBy(dx: 10.5, dy: 10.5)
            ctx.setStrokeColor(theme.jadeInlay)
            ctx.setLineWidth(1.8)
            addRoundedRect(ctx, rect: inset, radius: max(4, r - 5))
            ctx.strokePath()

            let cx: CGFloat = 50
            let cy = faceRect.midY
            ctx.beginPath()
            ctx.move(to: CGPoint(x: cx, y: cy - 22))
            ctx.addLine(to: CGPoint(x: cx + 15, y: cy))
            ctx.addLine(to: CGPoint(x: cx, y: cy + 22))
            ctx.addLine(to: CGPoint(x: cx - 15, y: cy))
            ctx.closePath()
            ctx.setLineWidth(1.8)
            ctx.strokePath()

            ctx.setFillColor(theme.jadeGoldDot)
            ctx.fillEllipse(in: CGRect(x: cx - 4.5, y: cy - 4.5, width: 9, height: 9))
            ctx.restoreGState()

            ctx.setStrokeColor(theme.jadeRimStroke)
            ctx.setLineWidth(1.0)
            addRoundedRect(ctx, rect: backBlock, radius: r)
            ctx.strokePath()
            ctx.restoreGState()
            return
        }

        // Standing front ivory face with subtle ivory-to-jade bevel seam
        let seamRect = CGRect(x: 2.0, y: 1.5 + theme.standingTopCap - 2.6, width: 96.0, height: 6.0)
        ctx.saveGState()
        addRoundedRect(ctx, rect: backBlock, radius: r)
        ctx.clip()
        ctx.setFillColor(rgba(0x042B1E, 0.34))
        ctx.fill(seamRect)
        ctx.restoreGState()

        let faceRect = faceRectFor(laid: false, theme: theme)
        ctx.saveGState()
        addRoundedRect(ctx, rect: faceRect, radius: r - 0.5)
        ctx.clip()
        drawLinearGrad(
            ctx,
            colors: [theme.ivoryTop, theme.ivoryMid, theme.ivoryBot],
            locations: [0.0, 0.72, 1.0],
            start: CGPoint(x: 50, y: faceRect.minY),
            end: CGPoint(x: 50, y: faceRect.maxY)
        )
        drawRadialGrad(
            ctx,
            colors: [rgba(0xFFFFFF, 0.42), rgba(0xFFFFFF, 0.0)],
            locations: [0.0, 1.0],
            center: CGPoint(x: 28, y: faceRect.minY + 18),
            startR: 2,
            endR: 58
        )
        ctx.setFillColor(theme.ivoryBevelLight)
        ctx.fill(CGRect(x: 4, y: faceRect.minY, width: 92, height: 3.2))
        drawLinearGrad(
            ctx,
            colors: [rgba(0x705D42, 0.0), theme.ivoryBevelShadow],
            locations: [0.0, 1.0],
            start: CGPoint(x: 50, y: faceRect.maxY - 8.5),
            end: CGPoint(x: 50, y: faceRect.maxY)
        )
        drawLinearGrad(
            ctx,
            colors: [rgba(0xFFFFFF, 0.38), rgba(0xFFFFFF, 0.0)],
            locations: [0.0, 1.0],
            start: CGPoint(x: faceRect.minX, y: 60),
            end: CGPoint(x: faceRect.minX + 5.5, y: 60)
        )
        drawLinearGrad(
            ctx,
            colors: [rgba(0x6E5B42, 0.0), rgba(0x6E5B42, 0.16)],
            locations: [0.0, 1.0],
            start: CGPoint(x: faceRect.maxX - 5.5, y: 60),
            end: CGPoint(x: faceRect.maxX, y: 60)
        )
        ctx.restoreGState()

        ctx.setStrokeColor(theme.ivoryRimStroke)
        ctx.setLineWidth(0.95)
        addRoundedRect(ctx, rect: faceRect, radius: r - 0.5)
        ctx.strokePath()

        ctx.setStrokeColor(theme.jadeRimStroke)
        ctx.setLineWidth(0.85)
        addRoundedRect(ctx, rect: backBlock, radius: r)
        ctx.strokePath()

        ctx.restoreGState()
        return
    }

    // Laid tile: dual-layer 3D block (ivory side wall + emerald jade base lip)
    let fullBlock = CGRect(x: 2.0, y: 2.0, width: 96.0, height: 131.5)
    let faceRect = faceRectFor(laid: true, theme: theme)
    let wallBottomY = 133.5 - theme.laidJadeLip

    if isBack {
        ctx.saveGState()
        addRoundedRect(ctx, rect: fullBlock, radius: r)
        ctx.clip()
        drawLinearGrad(
            ctx,
            colors: [theme.ivoryWallTop, theme.ivoryWallBot],
            locations: [0.0, 1.0],
            start: CGPoint(x: 50, y: wallBottomY - 2),
            end: CGPoint(x: 50, y: 133.5)
        )
        ctx.restoreGState()

        let jadeSlab = CGRect(x: 2.0, y: 2.0, width: 96.0, height: wallBottomY - 2.0)
        ctx.saveGState()
        addRoundedRect(ctx, rect: jadeSlab, radius: r)
        ctx.clip()
        drawLinearGrad(
            ctx,
            colors: [theme.jadeMid, theme.jadeDeep],
            locations: [0.0, 1.0],
            start: CGPoint(x: 50, y: faceRect.maxY - 2),
            end: CGPoint(x: 50, y: wallBottomY)
        )
        ctx.restoreGState()

        ctx.saveGState()
        addRoundedRect(ctx, rect: faceRect, radius: r - 0.5)
        ctx.clip()
        drawLinearGrad(
            ctx,
            colors: [theme.jadeTopHi, theme.jadeMid, theme.jadeDeep],
            locations: [0.0, 0.55, 1.0],
            start: CGPoint(x: 15, y: faceRect.minY),
            end: CGPoint(x: 85, y: faceRect.maxY)
        )
        let inset = faceRect.insetBy(dx: 10.5, dy: 9.5)
        ctx.setStrokeColor(theme.jadeInlay)
        ctx.setLineWidth(1.7)
        addRoundedRect(ctx, rect: inset, radius: max(4, r - 5))
        ctx.strokePath()
        let cx: CGFloat = 50
        let cy = faceRect.midY
        ctx.beginPath()
        ctx.move(to: CGPoint(x: cx, y: cy - 18))
        ctx.addLine(to: CGPoint(x: cx + 13, y: cy))
        ctx.addLine(to: CGPoint(x: cx, y: cy + 18))
        ctx.addLine(to: CGPoint(x: cx - 13, y: cy))
        ctx.closePath()
        ctx.strokePath()
        ctx.setFillColor(theme.jadeGoldDot)
        ctx.fillEllipse(in: CGRect(x: cx - 4, y: cy - 4, width: 8, height: 8))
        ctx.restoreGState()

        ctx.setStrokeColor(theme.jadeRimStroke)
        ctx.setLineWidth(0.95)
        addRoundedRect(ctx, rect: fullBlock, radius: r)
        ctx.strokePath()
        ctx.restoreGState()
        return
    }

    // 1. Bottom emerald jade base lip
    ctx.saveGState()
    addRoundedRect(ctx, rect: fullBlock, radius: r)
    ctx.clip()
    drawLinearGrad(
        ctx,
        colors: [theme.jadeMid, theme.jadeDeep, rgba(0x04291D)],
        locations: [0.0, 0.6, 1.0],
        start: CGPoint(x: 50, y: wallBottomY - 3),
        end: CGPoint(x: 50, y: 133.5)
    )
    ctx.setFillColor(rgba(0x031F16, 0.32))
    ctx.fill(CGRect(x: 2, y: wallBottomY - 0.5, width: 96, height: 1.8))
    ctx.restoreGState()

    // 2. Middle ivory vertical side wall
    let ivorySlab = CGRect(x: 2.0, y: 2.0, width: 96.0, height: wallBottomY - 2.0)
    ctx.saveGState()
    addRoundedRect(ctx, rect: ivorySlab, radius: r - 0.2)
    ctx.clip()
    drawLinearGrad(
        ctx,
        colors: [theme.ivoryWallTop, theme.ivoryWallBot],
        locations: [0.0, 1.0],
        start: CGPoint(x: 50, y: faceRect.maxY - 2),
        end: CGPoint(x: 50, y: wallBottomY)
    )
    drawLinearGrad(
        ctx,
        colors: [rgba(0x705B3E, 0.22), rgba(0x705B3E, 0.0), rgba(0x705B3E, 0.0), rgba(0x5A472F, 0.26)],
        locations: [0.0, 0.14, 0.86, 1.0],
        start: CGPoint(x: 2, y: wallBottomY - 5),
        end: CGPoint(x: 98, y: wallBottomY - 5)
    )
    ctx.restoreGState()

    // 3. Top engraved ivory face
    ctx.saveGState()
    addRoundedRect(ctx, rect: faceRect, radius: r - 0.6)
    ctx.clip()
    drawLinearGrad(
        ctx,
        colors: [theme.ivoryTop, theme.ivoryMid, theme.ivoryBot],
        locations: [0.0, 0.78, 1.0],
        start: CGPoint(x: 50, y: faceRect.minY),
        end: CGPoint(x: 50, y: faceRect.maxY)
    )
    drawRadialGrad(
        ctx,
        colors: [rgba(0xFFFFFF, 0.38), rgba(0xFFFFFF, 0.0)],
        locations: [0.0, 1.0],
        center: CGPoint(x: 32, y: faceRect.minY + 20),
        startR: 2,
        endR: 56
    )
    ctx.setFillColor(rgba(0xFFFFFF, 0.76))
    ctx.fill(CGRect(x: 6, y: faceRect.maxY - 2.3, width: 88, height: 1.8))
    ctx.setFillColor(theme.ivoryBevelLight)
    ctx.fill(CGRect(x: 6, y: faceRect.minY, width: 88, height: 2.2))
    ctx.restoreGState()

    ctx.setStrokeColor(theme.ivoryRimStroke)
    ctx.setLineWidth(0.75)
    addRoundedRect(ctx, rect: faceRect.insetBy(dx: 0.35, dy: 0.35), radius: r - 0.8)
    ctx.strokePath()

    ctx.setStrokeColor(rgba(0x0B2B21, 0.36))
    ctx.setLineWidth(0.85)
    addRoundedRect(ctx, rect: fullBlock, radius: r)
    ctx.strokePath()

    ctx.restoreGState()
}

func resolveFont(_ primary: String, fallback: String, size: CGFloat) -> CTFont {
    let f = CTFontCreateWithName(primary as CFString, size, nil)
    if (CTFontCopyPostScriptName(f) as String) == primary {
        return f
    }
    return CTFontCreateWithName(fallback as CFString, size, nil)
}

func drawEngravedGlyph(
    _ ctx: CGContext,
    text: String,
    cx: CGFloat,
    cy: CGFloat,
    fontSize: CGFloat,
    fill: CGColor,
    theme: TileTheme,
    scaleX: CGFloat = 1.0,
    scaleY: CGFloat = 1.0
) {
    let font = resolveFont(theme.fontPS, fallback: theme.fallbackFontPS, size: fontSize)

    func renderPass(dx: CGFloat, dy: CGFloat, color: CGColor, strokePct: CGFloat) {
        var attrs: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): color
        ]
        if strokePct != 0 {
            attrs[NSAttributedString.Key(kCTStrokeWidthAttributeName as String)] = strokePct as NSNumber
            attrs[NSAttributedString.Key(kCTStrokeColorAttributeName as String)] = color
        }
        let attrStr = NSAttributedString(string: text, attributes: attrs)
        let line = CTLineCreateWithAttributedString(attrStr)
        let bounds = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)

        ctx.saveGState()
        ctx.translateBy(x: cx + dx, y: cy + dy)
        ctx.scaleBy(x: scaleX, y: -scaleY)
        ctx.setLineJoin(.round)
        ctx.textMatrix = .identity
        ctx.textPosition = CGPoint(x: -bounds.midX, y: -bounds.midY)
        CTLineDraw(line, ctx)
        ctx.restoreGState()
    }

    // 1. Intaglio recessed lower-right ivory catch-light
    renderPass(dx: 0.7, dy: 1.05, color: rgba(0xFFFFFF, 0.78), strokePct: theme.strokeWidthPct)
    // 2. Subtle upper-left carved depth shade
    renderPass(dx: -0.35, dy: -0.45, color: rgba(0x0E1014, 0.22), strokePct: theme.strokeWidthPct)
    // 3. Primary filled + stroke-thickened lacquer glyph
    renderPass(dx: 0, dy: 0, color: fill, strokePct: theme.strokeWidthPct)
}

func drawCarvedBambooStalk(
    _ ctx: CGContext,
    cx: CGFloat,
    cy: CGFloat,
    h: CGFloat,
    bw: CGFloat,
    color: CGColor,
    angleDeg: CGFloat = 0,
    theme: TileTheme
) {
    ctx.saveGState()
    ctx.translateBy(x: cx, y: cy)
    ctx.rotate(by: angleDeg * .pi / 180.0)

    let halfH = h * 0.5
    let stemW = bw * 0.74
    let nodeW = bw * 1.18
    let nodeH = max(4.2, min(6.2, h * 0.145))

    func strokeFillBody(dx: CGFloat, dy: CGFloat, fillColor: CGColor) {
        ctx.saveGState()
        ctx.translateBy(x: dx, y: dy)
        ctx.setFillColor(fillColor)

        let waistW = stemW * 0.78
        let p = CGMutablePath()
        p.move(to: CGPoint(x: -stemW * 0.5, y: -halfH + 2.2))
        p.addQuadCurve(to: CGPoint(x: -stemW * 0.5, y: 0), control: CGPoint(x: -waistW * 0.45, y: -halfH * 0.5))
        p.addQuadCurve(to: CGPoint(x: -stemW * 0.5, y: halfH - 2.2), control: CGPoint(x: -waistW * 0.45, y: halfH * 0.5))
        p.addLine(to: CGPoint(x: stemW * 0.5, y: halfH - 2.2))
        p.addQuadCurve(to: CGPoint(x: stemW * 0.5, y: 0), control: CGPoint(x: waistW * 0.45, y: halfH * 0.5))
        p.addQuadCurve(to: CGPoint(x: stemW * 0.5, y: -halfH + 2.2), control: CGPoint(x: waistW * 0.45, y: -halfH * 0.5))
        p.closeSubpath()
        ctx.addPath(p)
        ctx.fillPath()

        let topNode = CGRect(x: -nodeW * 0.5, y: -halfH, width: nodeW, height: nodeH)
        let midNode = CGRect(x: -nodeW * 0.54, y: -nodeH * 0.52, width: nodeW * 1.08, height: nodeH * 1.04)
        let botNode = CGRect(x: -nodeW * 0.5, y: halfH - nodeH, width: nodeW, height: nodeH)
        for rect in [topNode, midNode, botNode] {
            addRoundedRect(ctx, rect: rect, radius: nodeH * 0.48)
            ctx.fillPath()
        }
        ctx.restoreGState()
    }

    strokeFillBody(dx: 0.7, dy: 0.95, fillColor: rgba(0xFFFFFF, 0.75))
    strokeFillBody(dx: 0, dy: 0, fillColor: color)

    let ivoryCut = theme.ivoryTop
    ctx.setStrokeColor(ivoryCut)
    ctx.setLineCap(.round)
    ctx.setLineWidth(max(1.5, bw * 0.16))

    let segGap = nodeH * 0.65
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 0, y: -halfH + nodeH * 0.85))
    ctx.addLine(to: CGPoint(x: 0, y: -segGap))
    ctx.move(to: CGPoint(x: 0, y: segGap))
    ctx.addLine(to: CGPoint(x: 0, y: halfH - nodeH * 0.85))
    ctx.strokePath()

    ctx.setLineWidth(max(1.2, bw * 0.12))
    ctx.beginPath()
    ctx.move(to: CGPoint(x: -nodeW * 0.28, y: 0))
    ctx.addLine(to: CGPoint(x: nodeW * 0.28, y: 0))
    ctx.strokePath()

    ctx.setFillColor(ivoryCut)
    let dotR = max(0.85, bw * 0.085)
    ctx.fillEllipse(in: CGRect(x: -dotR, y: -halfH + nodeH * 0.5 - dotR, width: dotR * 2, height: dotR * 2))
    ctx.fillEllipse(in: CGRect(x: -dotR, y: halfH - nodeH * 0.5 - dotR, width: dotR * 2, height: dotR * 2))

    ctx.restoreGState()
}

func drawCarvedPin(
    _ ctx: CGContext,
    cx: CGFloat,
    cy: CGFloat,
    r: CGFloat,
    color: CGColor,
    theme: TileTheme
) {
    ctx.saveGState()

    ctx.setFillColor(rgba(0xFFFFFF, 0.78))
    ctx.fillEllipse(in: CGRect(x: cx - r + 0.65, y: cy - r + 0.95, width: r * 2, height: r * 2))

    ctx.setFillColor(color)
    ctx.fillEllipse(in: CGRect(x: cx - r, y: cy - r, width: r * 2, height: r * 2))

    let ringR = r * 0.77
    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(max(1.4, r * 0.115))
    ctx.strokeEllipse(in: CGRect(x: cx - ringR, y: cy - ringR, width: ringR * 2, height: ringR * 2))

    let petalCount = 6
    let petalOffset: CGFloat = 0.42
    let rx = r * 0.145
    let ry = r * 0.235

    ctx.setFillColor(theme.ivoryTop)
    for i in 0..<petalCount {
        let deg = Double(i) * (360.0 / Double(petalCount))
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.rotate(by: CGFloat(deg) * .pi / 180.0)
        ctx.fillEllipse(in: CGRect(x: -rx, y: -r * petalOffset - ry, width: rx * 2, height: ry * 2))
        ctx.restoreGState()
    }

    let hubR = r * 0.21
    ctx.setFillColor(color)
    ctx.fillEllipse(in: CGRect(x: cx - hubR, y: cy - hubR, width: hubR * 2, height: hubR * 2))

    let coreR = r * 0.095
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: cx - coreR, y: cy - coreR, width: coreR * 2, height: coreR * 2))

    ctx.restoreGState()
}

func drawEngravedOneBamboo(_ ctx: CGContext, theme: TileTheme) {
    ctx.saveGState()

    func drawBranch(dx: CGFloat, dy: CGFloat, color: CGColor, leafColor: CGColor) {
        ctx.saveGState()
        ctx.translateBy(x: dx, y: dy)
        ctx.setStrokeColor(color)
        ctx.setLineWidth(4.4)
        ctx.setLineCap(.round)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 16, y: 98))
        ctx.addQuadCurve(to: CGPoint(x: 84, y: 74), control: CGPoint(x: 50, y: 94))
        ctx.strokePath()

        let leaves: [(CGPoint, CGFloat, CGFloat, CGFloat)] = [
            (CGPoint(x: 23, y: 94), -2.55, 16, 4.2),
            (CGPoint(x: 34, y: 92), -1.85, 14, 3.8),
            (CGPoint(x: 78, y: 76), -0.55, 15, 4.0),
            (CGPoint(x: 71, y: 79), 0.35, 13, 3.6),
        ]
        ctx.setFillColor(leafColor)
        for (origin, ang, len, wid) in leaves {
            ctx.saveGState()
            ctx.translateBy(x: origin.x, y: origin.y)
            ctx.rotate(by: ang)
            let lp = CGMutablePath()
            lp.move(to: .zero)
            lp.addQuadCurve(to: CGPoint(x: len, y: 0), control: CGPoint(x: len * 0.45, y: -wid))
            lp.addQuadCurve(to: .zero, control: CGPoint(x: len * 0.45, y: wid))
            ctx.addPath(lp)
            ctx.fillPath()
            ctx.restoreGState()
        }
        ctx.restoreGState()
    }

    drawBranch(dx: 0.7, dy: 1.0, color: rgba(0xFFFFFF, 0.75), leafColor: rgba(0xFFFFFF, 0.75))
    drawBranch(dx: 0, dy: 0, color: theme.jade, leafColor: theme.jade)

    ctx.setStrokeColor(theme.crimson)
    ctx.setLineWidth(2.8)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 47, y: 74))
    ctx.addLine(to: CGPoint(x: 44, y: 89))
    ctx.move(to: CGPoint(x: 56, y: 72))
    ctx.addLine(to: CGPoint(x: 55, y: 86))
    ctx.strokePath()

    let tailFeathers: [(CGFloat, CGFloat, CGColor, CGFloat)] = [
        (0.32, 32, theme.crimson, 4.8),
        (0.52, 37, theme.jade, 5.4),
        (0.72, 40, theme.cobalt, 5.4),
        (0.92, 36, theme.jade, 5.0),
        (1.12, 30, theme.crimson, 4.6),
    ]
    for (ang, len, col, wid) in tailFeathers {
        ctx.saveGState()
        ctx.translateBy(x: 58, y: 65)
        ctx.rotate(by: ang)
        let tp = CGMutablePath()
        tp.move(to: .zero)
        tp.addQuadCurve(to: CGPoint(x: len, y: 0), control: CGPoint(x: len * 0.55, y: -wid))
        tp.addQuadCurve(to: .zero, control: CGPoint(x: len * 0.55, y: wid))
        ctx.setFillColor(col)
        ctx.addPath(tp)
        ctx.fillPath()
        ctx.setStrokeColor(theme.ivoryTop)
        ctx.setLineWidth(1.25)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 4, y: 0))
        ctx.addLine(to: CGPoint(x: len - 5, y: 0))
        ctx.strokePath()
        ctx.restoreGState()
    }

    let bodyPath = CGMutablePath()
    bodyPath.move(to: CGPoint(x: 34, y: 24))
    bodyPath.addCurve(to: CGPoint(x: 27, y: 55), control1: CGPoint(x: 27, y: 34), control2: CGPoint(x: 23, y: 45))
    bodyPath.addCurve(to: CGPoint(x: 52, y: 77), control1: CGPoint(x: 31, y: 69), control2: CGPoint(x: 42, y: 78))
    bodyPath.addCurve(to: CGPoint(x: 72, y: 60), control1: CGPoint(x: 63, y: 76), control2: CGPoint(x: 71, y: 69))
    bodyPath.addCurve(to: CGPoint(x: 53, y: 33), control1: CGPoint(x: 72, y: 47), control2: CGPoint(x: 62, y: 39))
    bodyPath.addCurve(to: CGPoint(x: 48, y: 20), control1: CGPoint(x: 51, y: 28), control2: CGPoint(x: 52, y: 23))
    bodyPath.addCurve(to: CGPoint(x: 34, y: 24), control1: CGPoint(x: 44, y: 17), control2: CGPoint(x: 37, y: 18))
    bodyPath.closeSubpath()

    ctx.saveGState()
    ctx.translateBy(x: 0.8, y: 1.1)
    ctx.setFillColor(rgba(0xFFFFFF, 0.78))
    ctx.addPath(bodyPath)
    ctx.fillPath()
    ctx.restoreGState()

    ctx.setFillColor(theme.jade)
    ctx.addPath(bodyPath)
    ctx.fillPath()

    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(2.0)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 34, y: 37))
    ctx.addQuadCurve(to: CGPoint(x: 48, y: 35), control: CGPoint(x: 41, y: 40))
    ctx.strokePath()

    let bellyPath = CGMutablePath()
    bellyPath.move(to: CGPoint(x: 32, y: 46))
    bellyPath.addQuadCurve(to: CGPoint(x: 46, y: 71), control: CGPoint(x: 30, y: 64))
    bellyPath.addQuadCurve(to: CGPoint(x: 44, y: 50), control: CGPoint(x: 48, y: 60))
    bellyPath.closeSubpath()
    ctx.setFillColor(theme.crimson)
    ctx.addPath(bellyPath)
    ctx.fillPath()

    let wingPath = CGMutablePath()
    wingPath.move(to: CGPoint(x: 44, y: 41))
    wingPath.addQuadCurve(to: CGPoint(x: 74, y: 58), control: CGPoint(x: 63, y: 39))
    wingPath.addQuadCurve(to: CGPoint(x: 51, y: 69), control: CGPoint(x: 66, y: 69))
    wingPath.addQuadCurve(to: CGPoint(x: 44, y: 41), control: CGPoint(x: 42, y: 55))
    wingPath.closeSubpath()
    ctx.setFillColor(theme.cobalt)
    ctx.addPath(wingPath)
    ctx.fillPath()

    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(1.65)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 49, y: 47))
    ctx.addQuadCurve(to: CGPoint(x: 66, y: 54), control: CGPoint(x: 58, y: 47))
    ctx.move(to: CGPoint(x: 49, y: 54))
    ctx.addQuadCurve(to: CGPoint(x: 67, y: 60), control: CGPoint(x: 58, y: 55))
    ctx.move(to: CGPoint(x: 51, y: 61))
    ctx.addQuadCurve(to: CGPoint(x: 63, y: 65), control: CGPoint(x: 57, y: 62))
    ctx.strokePath()

    let crestPath = CGMutablePath()
    crestPath.move(to: CGPoint(x: 34, y: 22))
    crestPath.addQuadCurve(to: CGPoint(x: 59, y: 10), control: CGPoint(x: 44, y: 10))
    crestPath.addQuadCurve(to: CGPoint(x: 49, y: 22), control: CGPoint(x: 53, y: 18))
    crestPath.addQuadCurve(to: CGPoint(x: 34, y: 22), control: CGPoint(x: 42, y: 21))
    crestPath.closeSubpath()
    ctx.setFillColor(theme.crimson)
    ctx.addPath(crestPath)
    ctx.fillPath()

    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(1.3)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 39, y: 19))
    ctx.addQuadCurve(to: CGPoint(x: 53, y: 13), control: CGPoint(x: 46, y: 15))
    ctx.strokePath()

    let beakPath = CGMutablePath()
    beakPath.move(to: CGPoint(x: 32, y: 24))
    beakPath.addLine(to: CGPoint(x: 18, y: 27.5))
    beakPath.addLine(to: CGPoint(x: 31, y: 31))
    beakPath.closeSubpath()
    ctx.setFillColor(theme.crimson)
    ctx.addPath(beakPath)
    ctx.fillPath()

    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: 33.5, y: 22.8, width: 6.4, height: 6.4))
    ctx.setFillColor(theme.ink)
    ctx.fillEllipse(in: CGRect(x: 35.1, y: 24.4, width: 3.3, height: 3.3))

    ctx.restoreGState()
}

func drawEngravedOneDot(_ ctx: CGContext, theme: TileTheme) {
    let cx: CGFloat = 50
    let cy: CGFloat = 56
    let outerR: CGFloat = 41.0

    ctx.saveGState()

    ctx.setFillColor(rgba(0xFFFFFF, 0.80))
    ctx.fillEllipse(in: CGRect(x: cx - outerR + 0.8, y: cy - outerR + 1.1, width: outerR * 2, height: outerR * 2))

    ctx.setFillColor(theme.cobalt)
    ctx.fillEllipse(in: CGRect(x: cx - outerR, y: cy - outerR, width: outerR * 2, height: outerR * 2))

    let beadCount = 16
    let beadOrbit = outerR * 0.855
    let beadR = outerR * 0.056
    ctx.setFillColor(theme.ivoryTop)
    for i in 0..<beadCount {
        let ang = CGFloat(i) * (2.0 * .pi / CGFloat(beadCount))
        let bx = cx + cos(ang) * beadOrbit
        let by = cy + sin(ang) * beadOrbit
        ctx.fillEllipse(in: CGRect(x: bx - beadR, y: by - beadR, width: beadR * 2, height: beadR * 2))
    }

    let sepR1 = outerR * 0.72
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: cx - sepR1, y: cy - sepR1, width: sepR1 * 2, height: sepR1 * 2))

    let wreathR = outerR * 0.66
    let wreathPetals = 12
    ctx.setFillColor(theme.jade)
    for i in 0..<wreathPetals {
        let ang = CGFloat(i) * (2.0 * .pi / CGFloat(wreathPetals))
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.rotate(by: ang)
        let pw = wreathR * 0.28
        let pp = CGMutablePath()
        pp.move(to: .zero)
        pp.addQuadCurve(to: CGPoint(x: 0, y: -wreathR), control: CGPoint(x: -pw, y: -wreathR * 0.62))
        pp.addQuadCurve(to: .zero, control: CGPoint(x: pw, y: -wreathR * 0.62))
        ctx.addPath(pp)
        ctx.fillPath()
        ctx.restoreGState()
    }

    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(1.45)
    for i in 0..<wreathPetals {
        let ang = (CGFloat(i) + 0.5) * (2.0 * .pi / CGFloat(wreathPetals))
        ctx.beginPath()
        ctx.move(to: CGPoint(x: cx, y: cy))
        ctx.addLine(to: CGPoint(x: cx + cos(ang) * wreathR, y: cy + sin(ang) * wreathR))
        ctx.strokePath()
    }

    let coreIvoryR = outerR * 0.41
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: cx - coreIvoryR, y: cy - coreIvoryR, width: coreIvoryR * 2, height: coreIvoryR * 2))

    let redR = outerR * 0.36
    ctx.setFillColor(theme.crimson)
    ctx.fillEllipse(in: CGRect(x: cx - redR, y: cy - redR, width: redR * 2, height: redR * 2))

    let innerPetals = 6
    ctx.setFillColor(theme.ivoryTop)
    for i in 0..<innerPetals {
        let ang = CGFloat(i) * (2.0 * .pi / CGFloat(innerPetals))
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.rotate(by: ang)
        let rx = redR * 0.20
        let ry = redR * 0.30
        ctx.fillEllipse(in: CGRect(x: -rx, y: -redR * 0.50 - ry, width: rx * 2, height: ry * 2))
        ctx.restoreGState()
    }

    let bullseyeR = redR * 0.25
    ctx.setFillColor(theme.crimson)
    ctx.fillEllipse(in: CGRect(x: cx - bullseyeR, y: cy - bullseyeR, width: bullseyeR * 2, height: bullseyeR * 2))

    let dotR = redR * 0.11
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: cx - dotR, y: cy - dotR, width: dotR * 2, height: dotR * 2))

    ctx.restoreGState()
}

func drawBlueFramedWhiteDragon(_ ctx: CGContext, theme: TileTheme) {
    ctx.saveGState()

    let outer = CGRect(x: 13.5, y: 10.0, width: 73.0, height: 92.0)
    let inner = outer.insetBy(dx: 7.5, dy: 7.5)

    ctx.setStrokeColor(rgba(0xFFFFFF, 0.82))
    ctx.setLineWidth(4.4)
    addRoundedRect(ctx, rect: outer.offsetBy(dx: 0.7, dy: 1.0), radius: 5.0)
    ctx.strokePath()

    ctx.setStrokeColor(theme.cobalt)
    ctx.setLineWidth(4.0)
    addRoundedRect(ctx, rect: outer, radius: 5.0)
    ctx.strokePath()

    ctx.setLineWidth(2.0)
    addRoundedRect(ctx, rect: inner, radius: 2.5)
    ctx.strokePath()

    let cornerSize: CGFloat = 11.5
    let corners: [(CGFloat, CGFloat)] = [
        (outer.minX, outer.minY),
        (outer.maxX - cornerSize, outer.minY),
        (outer.minX, outer.maxY - cornerSize),
        (outer.maxX - cornerSize, outer.maxY - cornerSize),
    ]
    for (cx, cy) in corners {
        let box = CGRect(x: cx, y: cy, width: cornerSize, height: cornerSize)
        ctx.setFillColor(theme.ivoryMid)
        ctx.fill(box)
        ctx.setStrokeColor(theme.cobalt)
        ctx.setLineWidth(2.4)
        ctx.stroke(box)
        ctx.setFillColor(theme.cobalt)
        ctx.fill(box.insetBy(dx: 3.6, dy: 3.6))
    }

    ctx.setFillColor(theme.cobalt)
    ctx.fill(CGRect(x: 42, y: outer.minY + 1.5, width: 16, height: 5.5))
    ctx.fill(CGRect(x: 42, y: outer.maxY - 7.0, width: 16, height: 5.5))
    ctx.fill(CGRect(x: outer.minX + 1.5, y: 47, width: 5.5, height: 18))
    ctx.fill(CGRect(x: outer.maxX - 7.0, y: 47, width: 5.5, height: 18))

    ctx.restoreGState()
}

func drawTileFace(_ ctx: CGContext, frame: CGRect, tileIndex: Int, laid: Bool, spec: StyleASpec, theme: TileTheme) {
    drawPhysicalBody(ctx, frame: frame, kind: laid ? "laid" : "standing", theme: theme)

    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    ctx.scaleBy(x: frame.width / 100.0, y: frame.height / 135.0)

    let face = faceRectFor(laid: laid, theme: theme)
    addRoundedRect(ctx, rect: face, radius: theme.cornerRadius - 0.6)
    ctx.clip()

    let insetX: CGFloat = 4.2
    let insetY: CGFloat = 5.0
    let targetW = face.width - insetX * 2
    let targetH = face.height - insetY * 2
    let uniformScale = min(targetW / 100.0, targetH / 112.0)
    let artW = 100.0 * uniformScale
    let artH = 112.0 * uniformScale
    let artX = face.midX - artW * 0.5
    let artY = face.midY - artH * 0.5

    ctx.translateBy(x: artX, y: artY)
    ctx.scaleBy(x: uniformScale, y: uniformScale)

    let cnNums = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"]

    if tileIndex < 9 {
        let n = tileIndex + 1
        let numY: CGFloat = n == 1 ? 28.5 : 27.5
        drawEngravedGlyph(ctx, text: cnNums[n], cx: 50, cy: numY, fontSize: theme.manNumeralSize, fill: theme.ink, theme: theme, scaleX: n == 1 ? 1.08 : 1.04, scaleY: 0.96)
        drawEngravedGlyph(ctx, text: "萬", cx: 50, cy: 82.0, fontSize: theme.manWanSize, fill: theme.crimson, theme: theme, scaleX: 1.05, scaleY: 0.98)
    } else if tileIndex < 18 {
        let rank = tileIndex - 9 + 1
        if rank == 1 {
            drawEngravedOneBamboo(ctx, theme: theme)
        } else if let entries = spec.bamboo[String(rank)] {
            let bw: CGFloat = rank <= 4 ? 16.2 : (rank <= 6 ? 14.8 : 12.6)
            for entry in entries {
                drawCarvedBambooStalk(
                    ctx,
                    cx: CGFloat(entry.x),
                    cy: CGFloat(entry.y),
                    h: CGFloat(entry.h),
                    bw: bw,
                    color: theme.resolve(entry.color),
                    angleDeg: CGFloat(entry.angle),
                    theme: theme
                )
            }
        }
    } else if tileIndex < 27 {
        let rank = tileIndex - 18 + 1
        if rank == 1 {
            drawEngravedOneDot(ctx, theme: theme)
        } else if let entries = spec.dot[String(rank)] {
            for entry in entries {
                drawCarvedPin(
                    ctx,
                    cx: CGFloat(entry.x),
                    cy: CGFloat(entry.y),
                    r: CGFloat(entry.r),
                    color: theme.resolve(entry.color),
                    theme: theme
                )
            }
        }
    } else if tileIndex == 33 {
        drawBlueFramedWhiteDragon(ctx, theme: theme)
    } else {
        let glyphs = [27: "東", 28: "南", 29: "西", 30: "北", 31: "中", 32: "發"]
        let color: CGColor = tileIndex == 31 ? theme.crimson : (tileIndex == 32 ? theme.jade : theme.ink)
        drawEngravedGlyph(ctx, text: glyphs[tileIndex] ?? "?", cx: 50, cy: 56, fontSize: theme.honorSize, fill: color, theme: theme, scaleX: 1.02, scaleY: 1.04)
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
    guard args.count >= 5 else {
        fputs("Usage: rasterize-assets.swift <atlas-png> <impasto-png> <brush-png> <spec-json>\n", stderr)
        exit(1)
    }

    let atlasUrl = URL(fileURLWithPath: args[1])
    let impastoUrl = URL(fileURLWithPath: args[2])
    let brushUrl = URL(fileURLWithPath: args[3])
    let specUrl = URL(fileURLWithPath: args[4])

    let specData = try Data(contentsOf: specUrl)
    let spec = try JSONDecoder().decode(StyleASpec.self, from: specData)
    let theme = TileTheme(spec: spec)

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
        drawTileFace(atlasCtx, frame: frameFor(slot: t), tileIndex: t, laid: false, spec: spec, theme: theme)
        drawTileFace(atlasCtx, frame: frameFor(slot: spec.grid.tileCount + t), tileIndex: t, laid: true, spec: spec, theme: theme)
    }

    for (idx, frameKey) in spec.grid.bodyFrames.enumerated() {
        let kind = frameKey.hasPrefix("body_") ? String(frameKey.dropFirst("body_".count)) : frameKey
        drawPhysicalBody(atlasCtx, frame: frameFor(slot: spec.grid.tileCount * 2 + idx), kind: kind, theme: theme)
    }

    try savePNG(ctx: atlasCtx, url: atlasUrl)
    try buildFeltImpasto(url: impastoUrl)
    try buildFeltBrush(url: brushUrl)
}

try main()
