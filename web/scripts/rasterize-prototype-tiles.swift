import Foundation
import CoreGraphics
import CoreText
import ImageIO
import UniformTypeIdentifiers

// Throwaway prototype rasterizer for #78:
// Generates 3 structurally distinct @2x traditional physical Chinese Mahjong tile atlases
// (10x8 grid, 192x256 cells, 184x248 frames, 1920x2048 <= 2048x2048).

enum VariantId: String, CaseIterable {
    case a
    case b
    case c
}

struct VariantTheme {
    let id: VariantId
    let fontPS: String
    let fallbackFontPS: String
    let strokeWidthPct: CGFloat // negative = CoreText fill+stroke thickening

    // Tile block geometry in normalized 100x135 space
    let cornerRadius: CGFloat
    let standingTopCap: CGFloat
    let laidIvoryWall: CGFloat
    let laidJadeLip: CGFloat
    let artScale: CGFloat // multiplier for inner artwork fill ratio

    // Surface colors
    let ivoryTop: CGColor
    let ivoryMid: CGColor
    let ivoryBot: CGColor
    let ivoryWallTop: CGColor
    let ivoryWallBot: CGColor
    let ivoryRimStroke: CGColor
    let ivoryBevelLight: CGColor
    let ivoryBevelShadow: CGColor

    // Emerald/jade back colors
    let jadeTopHi: CGColor
    let jadeMid: CGColor
    let jadeDeep: CGColor
    let jadeRimStroke: CGColor
    let jadeInlay: CGColor
    let jadeGoldDot: CGColor

    // Lacquer engraving inks
    let ink: CGColor
    let crimson: CGColor
    let jade: CGColor
    let cobalt: CGColor
    let goldAccent: CGColor
}

func rgba(_ hex: UInt32, _ alpha: CGFloat = 1.0) -> CGColor {
    let r = CGFloat((hex >> 16) & 0xFF) / 255.0
    let g = CGFloat((hex >> 8) & 0xFF) / 255.0
    let b = CGFloat(hex & 0xFF) / 255.0
    return CGColor(srgbRed: r, green: g, blue: b, alpha: alpha)
}

func themeFor(_ id: VariantId) -> VariantTheme {
    switch id {
    case .a:
        // Variant A — 国标机麻·厚玉雕版
        return VariantTheme(
            id: .a,
            fontPS: "WeibeiTC-Bold",
            fallbackFontPS: "STKaitiTC-Black",
            strokeWidthPct: -3.2,
            cornerRadius: 11.5,
            standingTopCap: 12.5,
            laidIvoryWall: 10.5,
            laidJadeLip: 9.5,
            artScale: 1.0,
            ivoryTop: rgba(0xFFFEFA),
            ivoryMid: rgba(0xF8F3E8),
            ivoryBot: rgba(0xECE2CE),
            ivoryWallTop: rgba(0xE7DBC5),
            ivoryWallBot: rgba(0xD3C2A3),
            ivoryRimStroke: rgba(0x524535, 0.28),
            ivoryBevelLight: rgba(0xFFFFFF, 0.88),
            ivoryBevelShadow: rgba(0x8D7B5E, 0.28),
            jadeTopHi: rgba(0x1EA67A),
            jadeMid: rgba(0x0E7A56),
            jadeDeep: rgba(0x074B34),
            jadeRimStroke: rgba(0x053324, 0.65),
            jadeInlay: rgba(0xB8F5DE, 0.42),
            jadeGoldDot: rgba(0xE8C06A, 0.68),
            ink: rgba(0x16181D),
            crimson: rgba(0xC81D1D),
            jade: rgba(0x0C7D49),
            cobalt: rgba(0x144692),
            goldAccent: rgba(0xD49B35)
        )
    case .b:
        // Variant B — 古法骨牌·暖象牙手刻
        return VariantTheme(
            id: .b,
            fontPS: "STXingkaiTC-Bold",
            fallbackFontPS: "STKaitiTC-Bold",
            strokeWidthPct: -2.8,
            cornerRadius: 13.8,
            standingTopCap: 11.5,
            laidIvoryWall: 10.0,
            laidJadeLip: 9.0,
            artScale: 0.99,
            ivoryTop: rgba(0xFBF3E2),
            ivoryMid: rgba(0xF3E6CC),
            ivoryBot: rgba(0xE2CFA9),
            ivoryWallTop: rgba(0xDDC69E),
            ivoryWallBot: rgba(0xC2A677),
            ivoryRimStroke: rgba(0x5E4A2E, 0.32),
            ivoryBevelLight: rgba(0xFFF9EC, 0.82),
            ivoryBevelShadow: rgba(0x836841, 0.34),
            jadeTopHi: rgba(0x217362),
            jadeMid: rgba(0x145245),
            jadeDeep: rgba(0x093028),
            jadeRimStroke: rgba(0x06211B, 0.72),
            jadeInlay: rgba(0xCBE7D6, 0.36),
            jadeGoldDot: rgba(0xD6AB56, 0.72),
            ink: rgba(0x221C16),
            crimson: rgba(0xAD231B),
            jade: rgba(0x14653E),
            cobalt: rgba(0x1A3E75),
            goldAccent: rgba(0xC28A32)
        )
    case .c:
        // Variant C — 精雕棋牌·极速高辨识
        return VariantTheme(
            id: .c,
            fontPS: "STKaitiTC-Black",
            fallbackFontPS: "SourceHanSerifSC-Heavy",
            strokeWidthPct: -1.8,
            cornerRadius: 9.0,
            standingTopCap: 8.5,
            laidIvoryWall: 7.5,
            laidJadeLip: 6.5,
            artScale: 1.07,
            ivoryTop: rgba(0xFFFFFF),
            ivoryMid: rgba(0xFCFBF7),
            ivoryBot: rgba(0xF1ECE0),
            ivoryWallTop: rgba(0xE6DFD0),
            ivoryWallBot: rgba(0xCFC5B3),
            ivoryRimStroke: rgba(0x3B3E44, 0.26),
            ivoryBevelLight: rgba(0xFFFFFF, 0.94),
            ivoryBevelShadow: rgba(0x7B7568, 0.24),
            jadeTopHi: rgba(0x1DA186),
            jadeMid: rgba(0x0D7560),
            jadeDeep: rgba(0x074A3D),
            jadeRimStroke: rgba(0x043027, 0.62),
            jadeInlay: rgba(0xC4F4E7, 0.44),
            jadeGoldDot: rgba(0xF2C96B, 0.70),
            ink: rgba(0x101318),
            crimson: rgba(0xD41717),
            jade: rgba(0x097A40),
            cobalt: rgba(0x0F46A1),
            goldAccent: rgba(0xE0A52E)
        )
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
        throw NSError(domain: "MajjjProto", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to make CGImage"])
    }
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
        throw NSError(domain: "MajjjProto", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to create PNG dest"])
    }
    CGImageDestinationAddImage(dest, image, nil)
    if !CGImageDestinationFinalize(dest) {
        throw NSError(domain: "MajjjProto", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to finalize PNG"])
    }
}

func addRoundedRect(_ ctx: CGContext, rect: CGRect, radius: CGFloat) {
    let r = min(radius, min(rect.width, rect.height) * 0.5)
    ctx.addPath(CGPath(roundedRect: rect, cornerWidth: r, cornerHeight: r, transform: nil))
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

// Returns the usable ivory face rect in 100x135 normalized coordinates.
func faceRectFor(laid: Bool, theme: VariantTheme) -> CGRect {
    if laid {
        let faceH = 133.5 - 2.0 - theme.laidJadeLip - theme.laidIvoryWall
        return CGRect(x: 2.0, y: 2.0, width: 96.0, height: faceH)
    } else {
        let y = 1.5 + theme.standingTopCap
        let faceH = 133.5 - y
        return CGRect(x: 2.0, y: y, width: 96.0, height: faceH)
    }
}

func drawPhysicalBody(_ ctx: CGContext, frame: CGRect, kind: String, theme: VariantTheme) {
    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    ctx.scaleBy(x: frame.width / 100.0, y: frame.height / 135.0)

    let r = theme.cornerRadius

    if kind == "shadow" {
        // Multi-step soft ambient + directional contact shadow
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
        // === STANDING TILE (自家 hand or opponent concealed back) ===
        // 1. Full thick emerald/jade back block extending above the ivory face
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
        // Top acrylic specular sheen along crown
        ctx.setFillColor(rgba(0xD8FFF0, 0.26))
        ctx.fill(CGRect(x: 6, y: 2.2, width: 88, height: 3.0))
        ctx.restoreGState()

        if isBack {
            // Opponent standing back: full jade block with recessed satin rim + traditional inlaid motif
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
            // Top chamfer catch-light
            ctx.setStrokeColor(rgba(0xD0FFEA, 0.30))
            ctx.setLineWidth(1.4)
            addRoundedRect(ctx, rect: faceRect.insetBy(dx: 1.0, dy: 1.0), radius: r - 0.8)
            ctx.strokePath()

            // Traditional inlaid double border + diamond emblem
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

        // Standing front ivory face
        // Subtle transitional ivory-to-jade bevel seam
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
        // Base vertical ceramic sheen
        drawLinearGrad(
            ctx,
            colors: [theme.ivoryTop, theme.ivoryMid, theme.ivoryBot],
            locations: [0.0, 0.72, 1.0],
            start: CGPoint(x: 50, y: faceRect.minY),
            end: CGPoint(x: 50, y: faceRect.maxY)
        )
        // Gentle upper-left specular acrylic gloss
        drawRadialGrad(
            ctx,
            colors: [rgba(0xFFFFFF, theme.id == .b ? 0.28 : 0.42), rgba(0xFFFFFF, 0.0)],
            locations: [0.0, 1.0],
            center: CGPoint(x: 28, y: faceRect.minY + 18),
            startR: 2,
            endR: 58
        )
        // Top beveled lip highlight
        ctx.setFillColor(theme.ivoryBevelLight)
        ctx.fill(CGRect(x: 4, y: faceRect.minY, width: 92, height: 3.2))
        // Bottom soft rounded edge shade
        drawLinearGrad(
            ctx,
            colors: [rgba(0x705D42, 0.0), theme.ivoryBevelShadow],
            locations: [0.0, 1.0],
            start: CGPoint(x: 50, y: faceRect.maxY - 8.5),
            end: CGPoint(x: 50, y: faceRect.maxY)
        )
        // Subtle recessed side edge bevels (left light, right soft shade)
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

        // Crisp physical hairline rim (no black cartoon outline!)
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

    // === LAID TILE (牌河 / 鸣牌) — TRUE 3D DUAL-LAYER BLOCK ===
    let fullBlock = CGRect(x: 2.0, y: 2.0, width: 96.0, height: 131.5)
    let faceRect = faceRectFor(laid: true, theme: theme)
    let wallBottomY = 133.5 - theme.laidJadeLip

    if isBack {
        // Upside-down laid tile: ivory underside lip at bottom, jade side wall, jade top back
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

    // 1. Bottom Emerald/Jade Base Lip (underneath ivory side wall)
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
    // Subtle dark seam between ivory wall and jade base
    ctx.setFillColor(rgba(0x031F16, 0.32))
    ctx.fill(CGRect(x: 2, y: wallBottomY - 0.5, width: 96, height: 1.8))
    ctx.restoreGState()

    // 2. Middle Ivory Vertical Side Wall (3D resin/bone slab thickness)
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
    // Gentle vertical wall shading on left/right edges for rounded cylinder corners
    drawLinearGrad(
        ctx,
        colors: [rgba(0x705B3E, 0.22), rgba(0x705B3E, 0.0), rgba(0x705B3E, 0.0), rgba(0x5A472F, 0.26)],
        locations: [0.0, 0.14, 0.86, 1.0],
        start: CGPoint(x: 2, y: wallBottomY - 5),
        end: CGPoint(x: 98, y: wallBottomY - 5)
    )
    ctx.restoreGState()

    // 3. Top Engraved Ivory Face
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
        colors: [rgba(0xFFFFFF, theme.id == .b ? 0.24 : 0.38), rgba(0xFFFFFF, 0.0)],
        locations: [0.0, 1.0],
        center: CGPoint(x: 32, y: faceRect.minY + 20),
        startR: 2,
        endR: 56
    )
    // Front chamfer catch-light separating top face from vertical ivory wall
    ctx.setFillColor(rgba(0xFFFFFF, 0.76))
    ctx.fill(CGRect(x: 6, y: faceRect.maxY - 2.3, width: 88, height: 1.8))
    // Top rim highlight
    ctx.setFillColor(theme.ivoryBevelLight)
    ctx.fill(CGRect(x: 6, y: faceRect.minY, width: 88, height: 2.2))
    ctx.restoreGState()

    // Crisp front-edge divider only along front lip + clean outer physical rim
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
    theme: VariantTheme,
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
    theme: VariantTheme
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

        // Concave-waisted sculpted bamboo shaft (two gentle curved segments)
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

        // Top, middle, bottom sculpted bamboo nodes
        let topNode = CGRect(x: -nodeW * 0.5, y: -halfH, width: nodeW, height: nodeH)
        let midNode = CGRect(x: -nodeW * 0.54, y: -nodeH * 0.52, width: nodeW * 1.08, height: nodeH * 1.04)
        let botNode = CGRect(x: -nodeW * 0.5, y: halfH - nodeH, width: nodeW, height: nodeH)
        for rect in [topNode, midNode, botNode] {
            addRoundedRect(ctx, rect: rect, radius: nodeH * 0.48)
            ctx.fillPath()
        }
        ctx.restoreGState()
    }

    // Recessed ivory catch-light shadow
    strokeFillBody(dx: 0.7, dy: 0.95, fillColor: rgba(0xFFFFFF, 0.75))
    // Main carved lacquer body
    strokeFillBody(dx: 0, dy: 0, fillColor: color)

    // Classic carved ivory interior split-groove & node notches (iconic physical mahjong bamboo!)
    let ivoryCut = theme.ivoryTop
    ctx.setStrokeColor(ivoryCut)
    ctx.setLineCap(.round)
    ctx.setLineWidth(max(1.5, bw * 0.16))

    // Upper & lower vertical carved channels
    let segGap = nodeH * 0.65
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 0, y: -halfH + nodeH * 0.85))
    ctx.addLine(to: CGPoint(x: 0, y: -segGap))
    ctx.move(to: CGPoint(x: 0, y: segGap))
    ctx.addLine(to: CGPoint(x: 0, y: halfH - nodeH * 0.85))
    ctx.strokePath()

    // Crisp center node ivory groove
    ctx.setLineWidth(max(1.2, bw * 0.12))
    ctx.beginPath()
    ctx.move(to: CGPoint(x: -nodeW * 0.28, y: 0))
    ctx.addLine(to: CGPoint(x: nodeW * 0.28, y: 0))
    ctx.strokePath()

    // Subtle top/bottom cap dots for Variant A & B extra craft
    if theme.id != .c {
        ctx.setFillColor(ivoryCut)
        let dotR = max(0.85, bw * 0.085)
        ctx.fillEllipse(in: CGRect(x: -dotR, y: -halfH + nodeH * 0.5 - dotR, width: dotR * 2, height: dotR * 2))
        ctx.fillEllipse(in: CGRect(x: -dotR, y: halfH - nodeH * 0.5 - dotR, width: dotR * 2, height: dotR * 2))
    }

    ctx.restoreGState()
}

func drawCarvedPin(
    _ ctx: CGContext,
    cx: CGFloat,
    cy: CGFloat,
    r: CGFloat,
    color: CGColor,
    theme: VariantTheme
) {
    ctx.saveGState()

    // 1. Recessed lower-right ivory catch-light
    ctx.setFillColor(rgba(0xFFFFFF, 0.78))
    ctx.fillEllipse(in: CGRect(x: cx - r + 0.65, y: cy - r + 0.95, width: r * 2, height: r * 2))

    // 2. Outer solid lacquer disc
    ctx.setFillColor(color)
    ctx.fillEllipse(in: CGRect(x: cx - r, y: cy - r, width: r * 2, height: r * 2))

    // 3. Outer carved ivory concentric groove
    let ringR = r * (theme.id == .c ? 0.74 : 0.77)
    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(max(1.4, r * (theme.id == .c ? 0.135 : 0.115)))
    ctx.strokeEllipse(in: CGRect(x: cx - ringR, y: cy - ringR, width: ringR * 2, height: ringR * 2))

    // 4. Variant-specific inner floral carving
    let petalCount = theme.id == .b ? 8 : (theme.id == .a ? 6 : 5)
    let petalOffset = theme.id == .c ? 0.39 : 0.42
    let rx = r * (theme.id == .b ? 0.11 : (theme.id == .a ? 0.145 : 0.175))
    let ry = r * (theme.id == .b ? 0.22 : (theme.id == .a ? 0.235 : 0.255))

    ctx.setFillColor(theme.ivoryTop)
    for i in 0..<petalCount {
        let deg = Double(i) * (360.0 / Double(petalCount))
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.rotate(by: CGFloat(deg) * .pi / 180.0)
        ctx.fillEllipse(in: CGRect(x: -rx, y: -r * petalOffset - ry, width: rx * 2, height: ry * 2))
        ctx.restoreGState()
    }

    // 5. Center colored hub + crisp ivory eye
    let hubR = r * (theme.id == .c ? 0.24 : 0.21)
    ctx.setFillColor(color)
    ctx.fillEllipse(in: CGRect(x: cx - hubR, y: cy - hubR, width: hubR * 2, height: hubR * 2))

    let coreR = r * (theme.id == .c ? 0.11 : 0.095)
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: cx - coreR, y: cy - coreR, width: coreR * 2, height: coreR * 2))

    ctx.restoreGState()
}

// Unified high-contrast engraved 1 条 (幺鸡 / 麻雀) drawn in matching carved lacquer style
func drawEngravedOneBamboo(_ ctx: CGContext, theme: VariantTheme) {
    ctx.saveGState()
    // Design space: 0..100 x 0..112 centered around (50, 56)

    // Subtle ivory catch-light offset helper
    func drawBranch(dx: CGFloat, dy: CGFloat, color: CGColor, leafColor: CGColor) {
        ctx.saveGState()
        ctx.translateBy(x: dx, y: dy)
        ctx.setStrokeColor(color)
        ctx.setLineWidth(theme.id == .c ? 5.2 : 4.4)
        ctx.setLineCap(.round)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 16, y: 98))
        ctx.addQuadCurve(to: CGPoint(x: 84, y: 74), control: CGPoint(x: 50, y: 94))
        ctx.strokePath()

        // Bamboo branch leaves
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

    // Perched legs & claws (crimson/ochre lacquer)
    ctx.setStrokeColor(theme.id == .b ? theme.goldAccent : theme.crimson)
    ctx.setLineWidth(2.8)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 47, y: 74))
    ctx.addLine(to: CGPoint(x: 44, y: 89))
    ctx.move(to: CGPoint(x: 56, y: 72))
    ctx.addLine(to: CGPoint(x: 55, y: 86))
    ctx.strokePath()

    // Sweeping peacock/sparrow tail fan (crimson + jade layered plumes)
    let tailFeathers: [(CGFloat, CGFloat, CGColor, CGFloat)] = theme.id == .c
        ? [
            (0.42, 34, theme.crimson, 6.2),
            (0.68, 38, theme.jade, 6.8),
            (0.95, 33, theme.crimson, 6.0),
          ]
        : [
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
        // Carved ivory plume rib
        ctx.setStrokeColor(theme.ivoryTop)
        ctx.setLineWidth(1.25)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 4, y: 0))
        ctx.addLine(to: CGPoint(x: len - 5, y: 0))
        ctx.strokePath()
        ctx.restoreGState()
    }

    // Proud S-curved bird torso & neck (emerald jade carved body)
    let bodyPath = CGMutablePath()
    bodyPath.move(to: CGPoint(x: 34, y: 24)) // crown base
    bodyPath.addCurve(to: CGPoint(x: 27, y: 55), control1: CGPoint(x: 27, y: 34), control2: CGPoint(x: 23, y: 45)) // chest
    bodyPath.addCurve(to: CGPoint(x: 52, y: 77), control1: CGPoint(x: 31, y: 69), control2: CGPoint(x: 42, y: 78)) // belly
    bodyPath.addCurve(to: CGPoint(x: 72, y: 60), control1: CGPoint(x: 63, y: 76), control2: CGPoint(x: 71, y: 69)) // lower back
    bodyPath.addCurve(to: CGPoint(x: 53, y: 33), control1: CGPoint(x: 72, y: 47), control2: CGPoint(x: 62, y: 39)) // shoulder
    bodyPath.addCurve(to: CGPoint(x: 48, y: 20), control1: CGPoint(x: 51, y: 28), control2: CGPoint(x: 52, y: 23)) // nape
    bodyPath.addCurve(to: CGPoint(x: 34, y: 24), control1: CGPoint(x: 44, y: 17), control2: CGPoint(x: 37, y: 18)) // head top
    bodyPath.closeSubpath()

    // Recessed ivory catch-light under body
    ctx.saveGState()
    ctx.translateBy(x: 0.8, y: 1.1)
    ctx.setFillColor(rgba(0xFFFFFF, 0.78))
    ctx.addPath(bodyPath)
    ctx.fillPath()
    ctx.restoreGState()

    ctx.setFillColor(theme.jade)
    ctx.addPath(bodyPath)
    ctx.fillPath()

    // Carved ivory collar & breast contour
    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(theme.id == .c ? 2.4 : 2.0)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 34, y: 37))
    ctx.addQuadCurve(to: CGPoint(x: 48, y: 35), control: CGPoint(x: 41, y: 40))
    ctx.strokePath()

    // Crimson breast patch (traditional mahjong 幺鸡 red belly accent!)
    let bellyPath = CGMutablePath()
    bellyPath.move(to: CGPoint(x: 32, y: 46))
    bellyPath.addQuadCurve(to: CGPoint(x: 46, y: 71), control: CGPoint(x: 30, y: 64))
    bellyPath.addQuadCurve(to: CGPoint(x: 44, y: 50), control: CGPoint(x: 48, y: 60))
    bellyPath.closeSubpath()
    ctx.setFillColor(theme.crimson)
    ctx.addPath(bellyPath)
    ctx.fillPath()

    // Sculpted Wing (cobalt upper coverts + carved ivory feather scallops)
    let wingPath = CGMutablePath()
    wingPath.move(to: CGPoint(x: 44, y: 41))
    wingPath.addQuadCurve(to: CGPoint(x: 74, y: 58), control: CGPoint(x: 63, y: 39))
    wingPath.addQuadCurve(to: CGPoint(x: 51, y: 69), control: CGPoint(x: 66, y: 69))
    wingPath.addQuadCurve(to: CGPoint(x: 44, y: 41), control: CGPoint(x: 42, y: 55))
    wingPath.closeSubpath()
    ctx.setFillColor(theme.cobalt)
    ctx.addPath(wingPath)
    ctx.fillPath()

    // Crisp carved ivory wing feather arcs
    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(theme.id == .c ? 2.0 : 1.65)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 49, y: 47))
    ctx.addQuadCurve(to: CGPoint(x: 66, y: 54), control: CGPoint(x: 58, y: 47))
    ctx.move(to: CGPoint(x: 49, y: 54))
    ctx.addQuadCurve(to: CGPoint(x: 67, y: 60), control: CGPoint(x: 58, y: 55))
    ctx.move(to: CGPoint(x: 51, y: 61))
    ctx.addQuadCurve(to: CGPoint(x: 63, y: 65), control: CGPoint(x: 57, y: 62))
    ctx.strokePath()

    // Iconic crimson swept head crest (凤冠 / 幺鸡红冠)
    let crestPath = CGMutablePath()
    crestPath.move(to: CGPoint(x: 34, y: 22))
    crestPath.addQuadCurve(to: CGPoint(x: 59, y: 10), control: CGPoint(x: 44, y: 10))
    crestPath.addQuadCurve(to: CGPoint(x: 49, y: 22), control: CGPoint(x: 53, y: 18))
    crestPath.addQuadCurve(to: CGPoint(x: 34, y: 22), control: CGPoint(x: 42, y: 21))
    crestPath.closeSubpath()
    ctx.setFillColor(theme.crimson)
    ctx.addPath(crestPath)
    ctx.fillPath()

    // Second smaller crest plume
    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(1.3)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 39, y: 19))
    ctx.addQuadCurve(to: CGPoint(x: 53, y: 13), control: CGPoint(x: 46, y: 15))
    ctx.strokePath()

    // Sharp pointed beak facing left
    let beakPath = CGMutablePath()
    beakPath.move(to: CGPoint(x: 32, y: 24))
    beakPath.addLine(to: CGPoint(x: 18, y: 27.5))
    beakPath.addLine(to: CGPoint(x: 31, y: 31))
    beakPath.closeSubpath()
    ctx.setFillColor(theme.id == .b ? theme.goldAccent : theme.crimson)
    ctx.addPath(beakPath)
    ctx.fillPath()

    // Crisp eye (ivory ring + dark ink pupil)
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: 33.5, y: 22.8, width: 6.4, height: 6.4))
    ctx.setFillColor(theme.ink)
    ctx.fillEllipse(in: CGRect(x: 35.1, y: 24.4, width: 3.3, height: 3.3))

    ctx.restoreGState()
}

// Unified high-contrast engraved 1 筒 (大花轮) drawn in matching carved lacquer style
func drawEngravedOneDot(_ ctx: CGContext, theme: VariantTheme) {
    let cx: CGFloat = 50
    let cy: CGFloat = 56
    let outerR: CGFloat = theme.id == .c ? 42.5 : (theme.id == .a ? 41.0 : 40.0)

    ctx.saveGState()

    // Recessed ivory catch-light
    ctx.setFillColor(rgba(0xFFFFFF, 0.80))
    ctx.fillEllipse(in: CGRect(x: cx - outerR + 0.8, y: cy - outerR + 1.1, width: outerR * 2, height: outerR * 2))

    // 1. Outer deep cobalt/indigo wheel disc
    ctx.setFillColor(theme.cobalt)
    ctx.fillEllipse(in: CGRect(x: cx - outerR, y: cy - outerR, width: outerR * 2, height: outerR * 2))

    // 2. Perimeter carved ivory lace/beads around the outer cobalt ring
    let beadCount = theme.id == .c ? 12 : (theme.id == .a ? 16 : 14)
    let beadOrbit = outerR * 0.855
    let beadR = outerR * (theme.id == .c ? 0.068 : 0.056)
    ctx.setFillColor(theme.ivoryTop)
    for i in 0..<beadCount {
        let ang = CGFloat(i) * (2.0 * .pi / CGFloat(beadCount))
        let bx = cx + cos(ang) * beadOrbit
        let by = cy + sin(ang) * beadOrbit
        ctx.fillEllipse(in: CGRect(x: bx - beadR, y: by - beadR, width: beadR * 2, height: beadR * 2))
    }

    // 3. Crisp ivory separation ring
    let sepR1 = outerR * 0.72
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: cx - sepR1, y: cy - sepR1, width: sepR1 * 2, height: sepR1 * 2))

    // 4. Emerald jade floral wreath (8 or 12 carved lotus petals)
    let wreathR = outerR * 0.66
    let wreathPetals = theme.id == .a ? 12 : 8
    ctx.setFillColor(theme.jade)
    for i in 0..<wreathPetals {
        let ang = CGFloat(i) * (2.0 * .pi / CGFloat(wreathPetals))
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.rotate(by: ang)
        let pw = wreathR * (wreathPetals == 12 ? 0.28 : 0.40)
        let pp = CGMutablePath()
        pp.move(to: .zero)
        pp.addQuadCurve(to: CGPoint(x: 0, y: -wreathR), control: CGPoint(x: -pw, y: -wreathR * 0.62))
        pp.addQuadCurve(to: .zero, control: CGPoint(x: pw, y: -wreathR * 0.62))
        ctx.addPath(pp)
        ctx.fillPath()
        ctx.restoreGState()
    }

    // Subtle ivory petal dividers
    ctx.setStrokeColor(theme.ivoryTop)
    ctx.setLineWidth(theme.id == .c ? 1.8 : 1.45)
    for i in 0..<wreathPetals {
        let ang = (CGFloat(i) + 0.5) * (2.0 * .pi / CGFloat(wreathPetals))
        ctx.beginPath()
        ctx.move(to: CGPoint(x: cx, y: cy))
        ctx.addLine(to: CGPoint(x: cx + cos(ang) * wreathR, y: cy + sin(ang) * wreathR))
        ctx.strokePath()
    }

    // Variant B warm gold-ochre ring accent
    if theme.id == .b {
        let goldR = outerR * 0.45
        ctx.setStrokeColor(theme.goldAccent)
        ctx.setLineWidth(2.4)
        ctx.strokeEllipse(in: CGRect(x: cx - goldR, y: cy - goldR, width: goldR * 2, height: goldR * 2))
    }

    // 5. Inner crimson core flower medallion
    let coreIvoryR = outerR * 0.41
    ctx.setFillColor(theme.ivoryTop)
    ctx.fillEllipse(in: CGRect(x: cx - coreIvoryR, y: cy - coreIvoryR, width: coreIvoryR * 2, height: coreIvoryR * 2))

    let redR = outerR * 0.36
    ctx.setFillColor(theme.crimson)
    ctx.fillEllipse(in: CGRect(x: cx - redR, y: cy - redR, width: redR * 2, height: redR * 2))

    // Carved 5- or 6-petal ivory rosette inside the crimson heart
    let innerPetals = theme.id == .c ? 5 : 6
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

// Classic Blue-Framed 白板 (amends ADR-0009; structurally distinct per variant!)
func drawBlueFramedWhiteDragon(_ ctx: CGContext, theme: VariantTheme) {
    ctx.saveGState()

    switch theme.id {
    case .a:
        // Variant A: Indigo double-line fretwork (回纹/云雷角) framed 白板
        let outer = CGRect(x: 13.5, y: 10.0, width: 73.0, height: 92.0)
        let inner = outer.insetBy(dx: 7.5, dy: 7.5)

        // Recessed ivory catch-light
        ctx.setStrokeColor(rgba(0xFFFFFF, 0.82))
        ctx.setLineWidth(4.4)
        addRoundedRect(ctx, rect: outer.offsetBy(dx: 0.7, dy: 1.0), radius: 5.0)
        ctx.strokePath()

        // Outer solid indigo frame
        ctx.setStrokeColor(theme.cobalt)
        ctx.setLineWidth(4.0)
        addRoundedRect(ctx, rect: outer, radius: 5.0)
        ctx.strokePath()

        // Inner fine indigo frame
        ctx.setLineWidth(2.0)
        addRoundedRect(ctx, rect: inner, radius: 2.5)
        ctx.strokePath()

        // Classic fretwork corner squares + mid-edge ticks connecting outer & inner
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

        // Mid-edge traditional fret bars (top, bottom, left, right)
        ctx.setFillColor(theme.cobalt)
        ctx.fill(CGRect(x: 42, y: outer.minY + 1.5, width: 16, height: 5.5))
        ctx.fill(CGRect(x: 42, y: outer.maxY - 7.0, width: 16, height: 5.5))
        ctx.fill(CGRect(x: outer.minX + 1.5, y: 47, width: 5.5, height: 18))
        ctx.fill(CGRect(x: outer.maxX - 7.0, y: 47, width: 5.5, height: 18))

    case .b:
        // Variant B: Corner-ornamented vintage blue-framed 白板 with beaded inner border
        let outer = CGRect(x: 14.5, y: 10.5, width: 71.0, height: 91.0)
        let inner = outer.insetBy(dx: 6.5, dy: 6.5)

        ctx.setStrokeColor(rgba(0xFFFFFF, 0.78))
        ctx.setLineWidth(3.6)
        addRoundedRect(ctx, rect: outer.offsetBy(dx: 0.7, dy: 1.0), radius: 8.0)
        ctx.strokePath()

        ctx.setStrokeColor(theme.cobalt)
        ctx.setLineWidth(3.2)
        addRoundedRect(ctx, rect: outer, radius: 8.0)
        ctx.strokePath()

        // Dashed/beaded inner vintage line
        ctx.saveGState()
        ctx.setLineWidth(2.1)
        ctx.setLineDash(phase: 0, lengths: [3.2, 3.0])
        addRoundedRect(ctx, rect: inner, radius: 4.5)
        ctx.strokePath()
        ctx.restoreGState()

        // Vintage inward-curved corner brackets
        let arm: CGFloat = 13.0
        ctx.setStrokeColor(theme.cobalt)
        ctx.setLineWidth(2.6)
        ctx.setLineCap(.round)
        let bracketSpecs: [(CGFloat, CGFloat, CGFloat, CGFloat)] = [
            (inner.minX + 2, inner.minY + 2, 1, 1),
            (inner.maxX - 2, inner.minY + 2, -1, 1),
            (inner.minX + 2, inner.maxY - 2, 1, -1),
            (inner.maxX - 2, inner.maxY - 2, -1, -1),
        ]
        for (bx, by, sx, sy) in bracketSpecs {
            ctx.beginPath()
            ctx.move(to: CGPoint(x: bx, y: by + sy * arm))
            ctx.addQuadCurve(to: CGPoint(x: bx + sx * arm, y: by), control: CGPoint(x: bx + sx * 4, y: by + sy * 4))
            ctx.strokePath()
            ctx.setFillColor(theme.cobalt)
            ctx.fillEllipse(in: CGRect(x: bx + sx * 3 - 2.2, y: by + sy * 3 - 2.2, width: 4.4, height: 4.4))
        }

    case .c:
        // Variant C: Clean beveled thick-stroke blue-framed 白板 with stepped notches
        let outer = CGRect(x: 11.5, y: 8.5, width: 77.0, height: 95.0)
        let inner = outer.insetBy(dx: 7.5, dy: 7.5)

        ctx.setStrokeColor(rgba(0xFFFFFF, 0.86))
        ctx.setLineWidth(5.4)
        addRoundedRect(ctx, rect: outer.offsetBy(dx: 0.7, dy: 1.0), radius: 5.5)
        ctx.strokePath()

        ctx.setStrokeColor(theme.cobalt)
        ctx.setLineWidth(5.0)
        addRoundedRect(ctx, rect: outer, radius: 5.5)
        ctx.strokePath()

        // Crisp inner thin recessed groove
        ctx.setStrokeColor(theme.cobalt)
        ctx.setAlpha(0.55)
        ctx.setLineWidth(1.6)
        addRoundedRect(ctx, rect: inner, radius: 3.0)
        ctx.strokePath()
        ctx.setAlpha(1.0)

        // Bold corner accent squares for instant glance readability
        let sq: CGFloat = 8.0
        ctx.setFillColor(theme.cobalt)
        for (x, y) in [
            (outer.minX + 1, outer.minY + 1),
            (outer.maxX - sq - 1, outer.minY + 1),
            (outer.minX + 1, outer.maxY - sq - 1),
            (outer.maxX - sq - 1, outer.maxY - sq - 1),
        ] {
            addRoundedRect(ctx, rect: CGRect(x: x, y: y, width: sq, height: sq), radius: 1.8)
            ctx.fillPath()
        }
    }

    ctx.restoreGState()
}

// Fuller, tighter traditional bamboo layouts in 100x112 face art space
struct StalkEntry {
    let x: CGFloat
    let y: CGFloat
    let h: CGFloat
    let color: String
    let angle: CGFloat
}

let BAMBOO_LAYOUTS: [Int: [StalkEntry]] = [
    2: [
        StalkEntry(x: 50, y: 31, h: 43, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 81, h: 43, color: "jade", angle: 0),
    ],
    3: [
        StalkEntry(x: 50, y: 30, h: 41, color: "jade", angle: 0),
        StalkEntry(x: 27, y: 81, h: 41, color: "jade", angle: 0),
        StalkEntry(x: 73, y: 81, h: 41, color: "jade", angle: 0),
    ],
    4: [
        StalkEntry(x: 28, y: 31, h: 42, color: "jade", angle: 0),
        StalkEntry(x: 72, y: 31, h: 42, color: "jade", angle: 0),
        StalkEntry(x: 28, y: 81, h: 42, color: "jade", angle: 0),
        StalkEntry(x: 72, y: 81, h: 42, color: "jade", angle: 0),
    ],
    5: [
        StalkEntry(x: 23, y: 29, h: 38, color: "jade", angle: 0),
        StalkEntry(x: 77, y: 29, h: 38, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 56, h: 38, color: "crimson", angle: 0),
        StalkEntry(x: 23, y: 83, h: 38, color: "jade", angle: 0),
        StalkEntry(x: 77, y: 83, h: 38, color: "jade", angle: 0),
    ],
    6: [
        StalkEntry(x: 23, y: 31, h: 41, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 31, h: 41, color: "jade", angle: 0),
        StalkEntry(x: 77, y: 31, h: 41, color: "jade", angle: 0),
        StalkEntry(x: 23, y: 81, h: 41, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 81, h: 41, color: "jade", angle: 0),
        StalkEntry(x: 77, y: 81, h: 41, color: "jade", angle: 0),
    ],
    7: [
        StalkEntry(x: 50, y: 21, h: 30, color: "crimson", angle: 0),
        StalkEntry(x: 23, y: 56, h: 31, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 56, h: 31, color: "jade", angle: 0),
        StalkEntry(x: 77, y: 56, h: 31, color: "jade", angle: 0),
        StalkEntry(x: 23, y: 91, h: 31, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 91, h: 31, color: "jade", angle: 0),
        StalkEntry(x: 77, y: 91, h: 31, color: "jade", angle: 0),
    ],
    8: [
        StalkEntry(x: 16.5, y: 31, h: 39, color: "jade", angle: 0),
        StalkEntry(x: 38.5, y: 33, h: 34, color: "jade", angle: 25),
        StalkEntry(x: 61.5, y: 33, h: 34, color: "jade", angle: -25),
        StalkEntry(x: 83.5, y: 31, h: 39, color: "jade", angle: 0),
        StalkEntry(x: 16.5, y: 81, h: 39, color: "jade", angle: 0),
        StalkEntry(x: 38.5, y: 79, h: 34, color: "jade", angle: -25),
        StalkEntry(x: 61.5, y: 79, h: 34, color: "jade", angle: 25),
        StalkEntry(x: 83.5, y: 81, h: 39, color: "jade", angle: 0),
    ],
    9: [
        StalkEntry(x: 22, y: 21, h: 30, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 21, h: 30, color: "crimson", angle: 0),
        StalkEntry(x: 78, y: 21, h: 30, color: "jade", angle: 0),
        StalkEntry(x: 22, y: 56, h: 30, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 56, h: 30, color: "crimson", angle: 0),
        StalkEntry(x: 78, y: 56, h: 30, color: "jade", angle: 0),
        StalkEntry(x: 22, y: 91, h: 30, color: "jade", angle: 0),
        StalkEntry(x: 50, y: 91, h: 30, color: "crimson", angle: 0),
        StalkEntry(x: 78, y: 91, h: 30, color: "jade", angle: 0),
    ],
]

// Fuller, tighter concentric floral discs in 100x112 face art space
struct PinEntry {
    let x: CGFloat
    let y: CGFloat
    let r: CGFloat
    let color: String
}

let PIN_LAYOUTS: [Int: [PinEntry]] = [
    2: [
        PinEntry(x: 50, y: 30, r: 22.5, color: "jade"),
        PinEntry(x: 50, y: 82, r: 22.5, color: "cobalt"),
    ],
    3: [
        PinEntry(x: 24, y: 25, r: 17.2, color: "cobalt"),
        PinEntry(x: 50, y: 56, r: 17.2, color: "crimson"),
        PinEntry(x: 76, y: 87, r: 17.2, color: "jade"),
    ],
    4: [
        PinEntry(x: 27, y: 31, r: 18.8, color: "cobalt"),
        PinEntry(x: 73, y: 31, r: 18.8, color: "jade"),
        PinEntry(x: 27, y: 81, r: 18.8, color: "jade"),
        PinEntry(x: 73, y: 81, r: 18.8, color: "cobalt"),
    ],
    5: [
        PinEntry(x: 24, y: 26, r: 15.6, color: "cobalt"),
        PinEntry(x: 76, y: 26, r: 15.6, color: "jade"),
        PinEntry(x: 50, y: 56, r: 16.0, color: "crimson"),
        PinEntry(x: 24, y: 86, r: 15.6, color: "jade"),
        PinEntry(x: 76, y: 86, r: 15.6, color: "cobalt"),
    ],
    6: [
        PinEntry(x: 29, y: 23, r: 14.6, color: "jade"),
        PinEntry(x: 71, y: 23, r: 14.6, color: "jade"),
        PinEntry(x: 29, y: 57, r: 14.6, color: "crimson"),
        PinEntry(x: 71, y: 57, r: 14.6, color: "crimson"),
        PinEntry(x: 29, y: 89, r: 14.6, color: "crimson"),
        PinEntry(x: 71, y: 89, r: 14.6, color: "crimson"),
    ],
    7: [
        PinEntry(x: 22, y: 19, r: 12.6, color: "jade"),
        PinEntry(x: 50, y: 29, r: 12.6, color: "jade"),
        PinEntry(x: 78, y: 39, r: 12.6, color: "jade"),
        PinEntry(x: 28, y: 65, r: 13.6, color: "crimson"),
        PinEntry(x: 72, y: 65, r: 13.6, color: "crimson"),
        PinEntry(x: 28, y: 93, r: 13.6, color: "crimson"),
        PinEntry(x: 72, y: 93, r: 13.6, color: "crimson"),
    ],
    8: [
        PinEntry(x: 29, y: 18.5, r: 12.6, color: "cobalt"),
        PinEntry(x: 71, y: 18.5, r: 12.6, color: "cobalt"),
        PinEntry(x: 29, y: 43.5, r: 12.6, color: "cobalt"),
        PinEntry(x: 71, y: 43.5, r: 12.6, color: "cobalt"),
        PinEntry(x: 29, y: 68.5, r: 12.6, color: "cobalt"),
        PinEntry(x: 71, y: 68.5, r: 12.6, color: "cobalt"),
        PinEntry(x: 29, y: 93.5, r: 12.6, color: "cobalt"),
        PinEntry(x: 71, y: 93.5, r: 12.6, color: "cobalt"),
    ],
    9: [
        PinEntry(x: 21.5, y: 22, r: 12.8, color: "jade"),
        PinEntry(x: 50.0, y: 22, r: 12.8, color: "jade"),
        PinEntry(x: 78.5, y: 22, r: 12.8, color: "jade"),
        PinEntry(x: 21.5, y: 56, r: 12.8, color: "crimson"),
        PinEntry(x: 50.0, y: 56, r: 12.8, color: "crimson"),
        PinEntry(x: 78.5, y: 56, r: 12.8, color: "crimson"),
        PinEntry(x: 21.5, y: 90, r: 12.8, color: "cobalt"),
        PinEntry(x: 50.0, y: 90, r: 12.8, color: "cobalt"),
        PinEntry(x: 78.5, y: 90, r: 12.8, color: "cobalt"),
    ],
]

func resolveColor(_ key: String, theme: VariantTheme) -> CGColor {
    switch key {
    case "crimson": return theme.crimson
    case "cobalt": return theme.cobalt
    case "jade": return theme.jade
    default: return theme.ink
    }
}

func drawTileFace(_ ctx: CGContext, frame: CGRect, tileIndex: Int, laid: Bool, theme: VariantTheme) {
    drawPhysicalBody(ctx, frame: frame, kind: laid ? "laid" : "standing", theme: theme)

    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    ctx.scaleBy(x: frame.width / 100.0, y: frame.height / 135.0)

    let face = faceRectFor(laid: laid, theme: theme)
    addRoundedRect(ctx, rect: face, radius: theme.cornerRadius - 0.6)
    ctx.clip()

    // Map 100x112 logical face art space cleanly into `face`, centered with Variant C artScale boost
    let insetX: CGFloat = theme.id == .c ? 2.5 : 4.2
    let insetY: CGFloat = theme.id == .c ? 3.2 : 5.0
    let targetW = face.width - insetX * 2
    let targetH = face.height - insetY * 2
    let baseScaleX = targetW / 100.0
    let baseScaleY = targetH / 112.0
    let uniformScale = min(baseScaleX, baseScaleY) * theme.artScale
    let artW = 100.0 * uniformScale
    let artH = 112.0 * uniformScale
    let artX = face.midX - artW * 0.5
    let artY = face.midY - artH * 0.5

    ctx.translateBy(x: artX, y: artY)
    ctx.scaleBy(x: uniformScale, y: uniformScale)

    let cnNums = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"]

    if tileIndex < 9 {
        // 萬子 (Characters) — heroically full-bleed engraved calligraphy
        let n = tileIndex + 1
        let numSize: CGFloat = theme.id == .b ? 57 : (theme.id == .c ? 56 : 53)
        let wanSize: CGFloat = theme.id == .b ? 59 : (theme.id == .c ? 58 : 55)
        let numY: CGFloat = n == 1 ? 28.5 : 27.5
        drawEngravedGlyph(ctx, text: cnNums[n], cx: 50, cy: numY, fontSize: numSize, fill: theme.ink, theme: theme, scaleX: n == 1 ? 1.08 : 1.04, scaleY: 0.96)
        drawEngravedGlyph(ctx, text: "萬", cx: 50, cy: 82.0, fontSize: wanSize, fill: theme.crimson, theme: theme, scaleX: 1.05, scaleY: 0.98)
    } else if tileIndex < 18 {
        // 条子 (Bamboos)
        let rank = tileIndex - 9 + 1
        if rank == 1 {
            drawEngravedOneBamboo(ctx, theme: theme)
        } else if let entries = BAMBOO_LAYOUTS[rank] {
            let baseBw: CGFloat = rank <= 4 ? 16.2 : (rank <= 6 ? 14.8 : 12.6)
            let bw = baseBw * (theme.id == .c ? 1.08 : 1.0)
            for e in entries {
                drawCarvedBambooStalk(
                    ctx,
                    cx: e.x,
                    cy: e.y,
                    h: e.h,
                    bw: bw,
                    color: resolveColor(e.color, theme: theme),
                    angleDeg: e.angle,
                    theme: theme
                )
            }
        }
    } else if tileIndex < 27 {
        // 筒子 (Dots)
        let rank = tileIndex - 18 + 1
        if rank == 1 {
            drawEngravedOneDot(ctx, theme: theme)
        } else if let entries = PIN_LAYOUTS[rank] {
            let rMul: CGFloat = theme.id == .c ? 1.05 : 1.0
            for e in entries {
                drawCarvedPin(
                    ctx,
                    cx: e.x,
                    cy: e.y,
                    r: e.r * rMul,
                    color: resolveColor(e.color, theme: theme),
                    theme: theme
                )
            }
        }
    } else if tileIndex == 33 {
        // 白板 — Classic blue-framed white dragon
        drawBlueFramedWhiteDragon(ctx, theme: theme)
    } else {
        // 字牌 (Winds & Dragons: 東南西北中發)
        let glyphs = [27: "東", 28: "南", 29: "西", 30: "北", 31: "中", 32: "發"]
        let color: CGColor = tileIndex == 31 ? theme.crimson : (tileIndex == 32 ? theme.jade : theme.ink)
        let fontSize: CGFloat = theme.id == .b ? 79 : (theme.id == .c ? 78 : 74)
        drawEngravedGlyph(ctx, text: glyphs[tileIndex] ?? "?", cx: 50, cy: 56, fontSize: fontSize, fill: color, theme: theme, scaleX: 1.02, scaleY: 1.04)
    }

    ctx.restoreGState()
}

func renderAtlas(variant: VariantId, outUrl: URL) throws {
    let theme = themeFor(variant)
    let cols = 10
    let rows = 8
    let cellW = 192
    let cellH = 256
    let frameW = 184
    let frameH = 248
    let pad = 4
    let atlasW = cols * cellW // 1920 <= 2048
    let atlasH = rows * cellH // 2048 <= 2048

    let ctx = makeContext(width: atlasW, height: atlasH)
    ctx.clear(CGRect(x: 0, y: 0, width: atlasW, height: atlasH))

    func frameFor(slot: Int) -> CGRect {
        let col = slot % cols
        let row = slot / cols
        return CGRect(x: col * cellW + pad, y: row * cellH + pad, width: frameW, height: frameH)
    }

    for t in 0..<34 {
        drawTileFace(ctx, frame: frameFor(slot: t), tileIndex: t, laid: false, theme: theme)
        drawTileFace(ctx, frame: frameFor(slot: 34 + t), tileIndex: t, laid: true, theme: theme)
    }

    let bodyKinds = ["standing", "laid", "back", "shadow", "laid_back"]
    for (idx, kind) in bodyKinds.enumerated() {
        drawPhysicalBody(ctx, frame: frameFor(slot: 68 + idx), kind: kind, theme: theme)
    }

    try savePNG(ctx: ctx, url: outUrl)
}

func main() throws {
    let args = CommandLine.arguments
    guard args.count >= 2 else {
        fputs("Usage: rasterize-prototype-tiles.swift <atlas-out-dir>\n", stderr)
        exit(1)
    }
    let outDir = URL(fileURLWithPath: args[1], isDirectory: true)
    for variant in VariantId.allCases {
        let outUrl = outDir.appendingPathComponent("prototype-tiles-\(variant.rawValue)@2x.png")
        try renderAtlas(variant: variant, outUrl: outUrl)
        print("Rasterized \(outUrl.lastPathComponent) (1920x2048, frame 184x248)")
    }
}

try main()
