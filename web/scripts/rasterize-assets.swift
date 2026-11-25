import Foundation
import CoreGraphics
import CoreText
import ImageIO
import UniformTypeIdentifiers

// Deterministic @2x vector-to-PNG rasterizer for Majjj Piltover tile atlas & impasto felt textures.

struct Palette {
    static let ink = CGColor(srgbRed: 0x18 / 255.0, green: 0x1A / 255.0, blue: 0x20 / 255.0, alpha: 1.0)
    static let crimson = CGColor(srgbRed: 0xC7 / 255.0, green: 0x35 / 255.0, blue: 0x2B / 255.0, alpha: 1.0)
    static let cobalt = CGColor(srgbRed: 0x1C / 255.0, green: 0x52 / 255.0, blue: 0x96 / 255.0, alpha: 1.0)
    static let jade = CGColor(srgbRed: 0x15 / 255.0, green: 0x7F / 255.0, blue: 0x56 / 255.0, alpha: 1.0)
    static let gold = CGColor(srgbRed: 0xC4 / 255.0, green: 0x8A / 255.0, blue: 0x28 / 255.0, alpha: 1.0)
    static let ivory = CGColor(srgbRed: 0xF5 / 255.0, green: 0xEE / 255.0, blue: 0xDF / 255.0, alpha: 1.0)
    static let ivoryShadow = CGColor(srgbRed: 0xE4 / 255.0, green: 0xD8 / 255.0, blue: 0xBE / 255.0, alpha: 1.0)
    static let ivorySpec = CGColor(srgbRed: 1.0, green: 0.99, blue: 0.96, alpha: 0.72)
    static let jadeBack = CGColor(srgbRed: 0x1E / 255.0, green: 0x5C / 255.0, blue: 0x5E / 255.0, alpha: 1.0)
    static let jadeBackHi = CGColor(srgbRed: 0x2A / 255.0, green: 0x76 / 255.0, blue: 0x78 / 255.0, alpha: 1.0)
    static let jadeBackDeep = CGColor(srgbRed: 0x14 / 255.0, green: 0x42 / 255.0, blue: 0x44 / 255.0, alpha: 1.0)
    static let jadeRim = CGColor(srgbRed: 0x39 / 255.0, green: 0x92 / 255.0, blue: 0x94 / 255.0, alpha: 0.85)
    static let white = CGColor(srgbRed: 1.0, green: 1.0, blue: 1.0, alpha: 1.0)
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
    // Flip to top-left origin
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
    let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    ctx.addPath(path)
}

func drawImpastoBody(_ ctx: CGContext, frame: CGRect, kind: String) {
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

    // Subtle cel depth shadow at bottom edge
    if !isLaid {
        let lipRect = CGRect(x: 2.5, y: 5.5, width: 95, height: 128)
        ctx.setFillColor(isBack ? Palette.jadeBackDeep : Palette.jadeBack)
        addRoundedRect(ctx, rect: lipRect, radius: 11)
        ctx.fillPath()
    }

    let bodyRect = CGRect(x: 2.5, y: 2.5, width: 95, height: isLaid ? 131 : 126)
    ctx.setFillColor(isBack ? Palette.jadeBack : Palette.ivory)
    addRoundedRect(ctx, rect: bodyRect, radius: 10.5)
    ctx.fillPath()

    ctx.saveGState()
    addRoundedRect(ctx, rect: bodyRect, radius: 10.5)
    ctx.clip()

    if isBack {
        // Subtle painterly brushstrokes on jade back only
        var rng = SeededRNG(seed: 918273)
        for _ in 0..<18 {
            let bx = rng.next() * 100.0
            let by = rng.next() * 136.0
            let bw = 14.0 + rng.next() * 24.0
            let bh = 3.0 + rng.next() * 5.0
            ctx.saveGState()
            ctx.translateBy(x: bx, y: by)
            ctx.rotate(by: -0.35 + (rng.next() - 0.5) * 0.3)
            ctx.setFillColor(rng.next() > 0.5 ? Palette.jadeBackHi : Palette.jadeBackDeep)
            ctx.setAlpha(0.32)
            addRoundedRect(ctx, rect: CGRect(x: -bw / 2, y: -bh / 2, width: bw, height: bh), radius: bh * 0.4)
            ctx.fillPath()
            ctx.restoreGState()
        }
        ctx.setAlpha(1.0)

        // Engraved inner frame + center diamond
        let inset = bodyRect.insetBy(dx: 10, dy: 11)
        ctx.setStrokeColor(Palette.jadeRim)
        ctx.setLineWidth(2.4)
        addRoundedRect(ctx, rect: inset, radius: 6)
        ctx.strokePath()

        let cx: CGFloat = 50
        let cy = bodyRect.midY
        ctx.beginPath()
        ctx.move(to: CGPoint(x: cx, y: cy - 16))
        ctx.addLine(to: CGPoint(x: cx + 12, y: cy))
        ctx.addLine(to: CGPoint(x: cx, y: cy + 16))
        ctx.addLine(to: CGPoint(x: cx - 12, y: cy))
        ctx.closePath()
        ctx.setLineWidth(2.2)
        ctx.strokePath()
    } else {
        // Crisp ivory top highlight + warm bevel bottom lip (zero noise in symbol zone!)
        ctx.setFillColor(Palette.ivorySpec)
        ctx.fill(CGRect(x: 4, y: 3, width: 92, height: 6))

        ctx.setFillColor(Palette.ivoryShadow)
        ctx.fill(CGRect(x: 4, y: bodyRect.maxY - (isLaid ? 6 : 9), width: 92, height: isLaid ? 6 : 9))
    }
    ctx.restoreGState()

    // Crisp comic ink outline
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(3.0)
    addRoundedRect(ctx, rect: bodyRect, radius: 10.5)
    ctx.strokePath()

    ctx.restoreGState()
}

func drawCenteredText(_ ctx: CGContext, text: String, x: CGFloat, y: CGFloat, fontSize: CGFloat, fill: CGColor, strokeWidth: CGFloat = 0) {
    let fontCandidates = ["Songti SC", "STSong", "PingFang SC", "Hiragino Sans GB"]
    var chosenFont: CTFont?
    for name in fontCandidates {
        let f = CTFontCreateWithName(name as CFString, fontSize, nil)
        if CTFontCopyFamilyName(f) as String == name || name == "PingFang SC" {
            chosenFont = f
            break
        }
    }
    let font = chosenFont ?? CTFontCreateWithName("PingFang SC" as CFString, fontSize, nil)

    var attrs: [NSAttributedString.Key: Any] = [
        NSAttributedString.Key(kCTFontAttributeName as String): font,
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): fill
    ]
    if strokeWidth > 0 {
        attrs[NSAttributedString.Key(kCTStrokeColorAttributeName as String)] = Palette.ink
        // Negative stroke width in CoreText means fill + stroke
        attrs[NSAttributedString.Key(kCTStrokeWidthAttributeName as String)] = -strokeWidth
    }

    let attrStr = NSAttributedString(string: text, attributes: attrs)
    let line = CTLineCreateWithAttributedString(attrStr)
    let bounds = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)

    ctx.saveGState()
    // Cancel top-left y-flip for CoreText text rendering
    ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
    let drawX = x - bounds.midX
    let drawY = y + bounds.height * 0.34
    ctx.textPosition = CGPoint(x: drawX, y: drawY)
    CTLineDraw(line, ctx)
    ctx.restoreGState()
}

func drawPinCircle(_ ctx: CGContext, cx: CGFloat, cy: CGFloat, r: CGFloat, fill: CGColor) {
    ctx.saveGState()
    let rect = CGRect(x: cx - r, y: cy - r, width: r * 2, height: r * 2)
    ctx.setFillColor(fill)
    ctx.fillEllipse(in: rect)
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(2.1)
    ctx.strokeEllipse(in: rect)

    let innerR = r * 0.56
    let innerRect = CGRect(x: cx - innerR, y: cy - innerR, width: innerR * 2, height: innerR * 2)
    ctx.setStrokeColor(Palette.ivory)
    ctx.setLineWidth(max(1.4, r * 0.14))
    ctx.strokeEllipse(in: innerRect)

    ctx.setLineCap(.round)
    ctx.setLineWidth(max(1.2, r * 0.12))
    for i in 0..<8 {
        let rad = CGFloat(i) * .pi / 4.0
        let x1 = cx + cos(rad) * (innerR + 1.2)
        let y1 = cy + sin(rad) * (innerR + 1.2)
        let x2 = cx + cos(rad) * (r - 2.2)
        let y2 = cy + sin(rad) * (r - 2.2)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: x1, y: y1))
        ctx.addLine(to: CGPoint(x: x2, y: y2))
        ctx.strokePath()
    }

    let dotR = r * 0.22
    let dotRect = CGRect(x: cx - dotR, y: cy - dotR, width: dotR * 2, height: dotR * 2)
    ctx.setFillColor(Palette.ivory)
    ctx.fillEllipse(in: dotRect)
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(1.2)
    ctx.strokeEllipse(in: dotRect)
    ctx.restoreGState()
}

func drawOnePinMedallion(_ ctx: CGContext) {
    ctx.saveGState()
    let outer = CGRect(x: 15, y: 31, width: 70, height: 70)
    ctx.setFillColor(Palette.cobalt)
    ctx.fillEllipse(in: outer)
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(2.8)
    ctx.strokeEllipse(in: outer)

    let ring = CGRect(x: 19.5, y: 35.5, width: 61, height: 61)
    ctx.setFillColor(Palette.ivory)
    ctx.fillEllipse(in: ring)
    ctx.setLineWidth(1.6)
    ctx.strokeEllipse(in: ring)

    for i in 0..<8 {
        ctx.saveGState()
        ctx.translateBy(x: 50, y: 66)
        ctx.rotate(by: CGFloat(i) * .pi / 4.0)
        let petal = CGRect(x: -6.6, y: -35, width: 13.2, height: 22)
        ctx.setFillColor(i % 2 == 0 ? Palette.jade : Palette.gold)
        ctx.fillEllipse(in: petal)
        ctx.setStrokeColor(Palette.ink)
        ctx.setLineWidth(1.8)
        ctx.strokeEllipse(in: petal)
        ctx.restoreGState()
    }

    let mid = CGRect(x: 36, y: 52, width: 28, height: 28)
    ctx.setFillColor(Palette.crimson)
    ctx.fillEllipse(in: mid)
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(2.2)
    ctx.strokeEllipse(in: mid)

    let core = CGRect(x: 42.5, y: 58.5, width: 15, height: 15)
    ctx.setFillColor(Palette.ivory)
    ctx.fillEllipse(in: core)
    ctx.setLineWidth(1.6)
    ctx.strokeEllipse(in: core)

    let dot = CGRect(x: 46.6, y: 62.6, width: 6.8, height: 6.8)
    ctx.setFillColor(Palette.crimson)
    ctx.fillEllipse(in: dot)
    ctx.restoreGState()
}

func drawBambooStalk(_ ctx: CGContext, cx: CGFloat, cy: CGFloat, h: CGFloat, color: CGColor, angleDeg: CGFloat = 0) {
    ctx.saveGState()
    ctx.translateBy(x: cx, y: cy)
    ctx.rotate(by: angleDeg * .pi / 180.0)
    let w: CGFloat = 11.5
    let top = -h * 0.5
    let bot = h * 0.5

    let stem = CGRect(x: -w * 0.44, y: top + 2, width: w * 0.88, height: h - 4)
    ctx.setFillColor(color)
    addRoundedRect(ctx, rect: stem, radius: 3.4)
    ctx.fillPath()
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(2.0)
    addRoundedRect(ctx, rect: stem, radius: 3.4)
    ctx.strokePath()

    for ny in [top - 1, -2.6, bot - 4.5] {
        let node = CGRect(x: -w * 0.56, y: ny, width: w * 1.12, height: 5.3)
        ctx.setFillColor(color)
        addRoundedRect(ctx, rect: node, radius: 2.5)
        ctx.fillPath()
        ctx.setStrokeColor(Palette.ink)
        ctx.setLineWidth(1.8)
        addRoundedRect(ctx, rect: node, radius: 2.5)
        ctx.strokePath()
    }

    ctx.setStrokeColor(Palette.ivory)
    ctx.setAlpha(0.85)
    ctx.setLineWidth(1.7)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 0, y: top + 5.5))
    ctx.addLine(to: CGPoint(x: 0, y: bot - 5.5))
    ctx.strokePath()
    ctx.restoreGState()
}

func drawOneBambooBird(_ ctx: CGContext) {
    ctx.saveGState()
    // Perch branch
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 18, y: 104))
    ctx.addQuadCurve(to: CGPoint(x: 82, y: 104), control: CGPoint(x: 50, y: 96))
    ctx.setStrokeColor(Palette.jade)
    ctx.setLineWidth(6)
    ctx.strokePath()
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 18, y: 104))
    ctx.addQuadCurve(to: CGPoint(x: 82, y: 104), control: CGPoint(x: 50, y: 96))
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(2)
    ctx.strokePath()

    // Tail feathers
    let tails: [([CGPoint], CGColor)] = [
        ([CGPoint(x: 40, y: 82), CGPoint(x: 22, y: 112), CGPoint(x: 34, y: 112), CGPoint(x: 46, y: 86)], Palette.jade),
        ([CGPoint(x: 48, y: 84), CGPoint(x: 42, y: 116), CGPoint(x: 54, y: 116), CGPoint(x: 55, y: 84)], Palette.crimson),
        ([CGPoint(x: 56, y: 82), CGPoint(x: 64, y: 112), CGPoint(x: 75, y: 109), CGPoint(x: 61, y: 80)], Palette.cobalt)
    ]
    ctx.setLineJoin(.round)
    for (pts, fill) in tails {
        ctx.beginPath()
        ctx.addLines(between: pts)
        ctx.closePath()
        ctx.setFillColor(fill)
        ctx.drawPath(using: .fillStroke)
    }

    // Body
    let body = CGRect(x: 33, y: 43, width: 36, height: 44)
    ctx.setFillColor(Palette.jade)
    ctx.fillEllipse(in: body)
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(2.4)
    ctx.strokeEllipse(in: body)

    let belly = CGRect(x: 42.5, y: 55, width: 21, height: 26)
    ctx.setFillColor(Palette.ivory)
    ctx.fillEllipse(in: belly)
    ctx.setLineWidth(1.6)
    ctx.strokeEllipse(in: belly)

    // Wings
    for wingPts in [
        (CGPoint(x: 35, y: 56), CGPoint(x: 27, y: 65), CGPoint(x: 30, y: 80), CGPoint(x: 44, y: 80)),
        (CGPoint(x: 67, y: 56), CGPoint(x: 75, y: 65), CGPoint(x: 72, y: 80), CGPoint(x: 58, y: 80))
    ] {
        ctx.beginPath()
        ctx.move(to: wingPts.0)
        ctx.addCurve(to: wingPts.3, control1: wingPts.1, control2: wingPts.2)
        ctx.closePath()
        ctx.setFillColor(Palette.cobalt)
        ctx.setStrokeColor(Palette.ink)
        ctx.setLineWidth(2.0)
        ctx.drawPath(using: .fillStroke)
    }

    // Head
    let head = CGRect(x: 39, y: 25, width: 26, height: 26)
    ctx.setFillColor(Palette.jade)
    ctx.fillEllipse(in: head)
    ctx.setStrokeColor(Palette.ink)
    ctx.setLineWidth(2.3)
    ctx.strokeEllipse(in: head)

    // Crest
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 46, y: 26))
    ctx.addQuadCurve(to: CGPoint(x: 53, y: 25), control: CGPoint(x: 46, y: 15))
    ctx.addQuadCurve(to: CGPoint(x: 58, y: 28), control: CGPoint(x: 64, y: 18))
    ctx.closePath()
    ctx.setFillColor(Palette.crimson)
    ctx.setLineWidth(1.9)
    ctx.drawPath(using: .fillStroke)

    // Beak
    ctx.beginPath()
    ctx.addLines(between: [CGPoint(x: 62, y: 37), CGPoint(x: 76, y: 40), CGPoint(x: 62, y: 44)])
    ctx.closePath()
    ctx.setFillColor(Palette.gold)
    ctx.setLineWidth(1.8)
    ctx.drawPath(using: .fillStroke)

    // Eye
    let eye = CGRect(x: 53, y: 34, width: 6, height: 6)
    ctx.setFillColor(Palette.ivory)
    ctx.fillEllipse(in: eye)
    ctx.setLineWidth(1.4)
    ctx.strokeEllipse(in: eye)
    ctx.setFillColor(Palette.ink)
    ctx.fillEllipse(in: CGRect(x: 55.4, y: 35.6, width: 2.8, height: 2.8))

    // Feet
    ctx.setLineWidth(2.4)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 45, y: 86)); ctx.addLine(to: CGPoint(x: 43, y: 100))
    ctx.move(to: CGPoint(x: 57, y: 86)); ctx.addLine(to: CGPoint(x: 59, y: 100))
    ctx.strokePath()
    ctx.restoreGState()
}

func drawTileFace(_ ctx: CGContext, frame: CGRect, tileIndex: Int) {
    drawImpastoBody(ctx, frame: frame, kind: "standing")

    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    ctx.scaleBy(x: frame.width / 100.0, y: frame.height / 136.0)

    let cnNums = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"]

    if tileIndex < 9 {
        // Man 1..9
        let n = tileIndex + 1
        drawCenteredText(ctx, text: cnNums[n], x: 50, y: 41, fontSize: 38, fill: Palette.ink, strokeWidth: 0.8)
        drawCenteredText(ctx, text: "萬", x: 50, y: 90, fontSize: 44, fill: Palette.crimson, strokeWidth: 1.4)
    } else if tileIndex < 18 {
        // Bamboo 1..9
        let n = tileIndex - 9 + 1
        switch n {
        case 1:
            drawOneBambooBird(ctx)
        case 2:
            drawBambooStalk(ctx, cx: 50, cy: 40, h: 38, color: Palette.jade)
            drawBambooStalk(ctx, cx: 50, cy: 90, h: 38, color: Palette.cobalt)
        case 3:
            drawBambooStalk(ctx, cx: 50, cy: 40, h: 36, color: Palette.jade)
            drawBambooStalk(ctx, cx: 32, cy: 90, h: 36, color: Palette.cobalt)
            drawBambooStalk(ctx, cx: 68, cy: 90, h: 36, color: Palette.jade)
        case 4:
            drawBambooStalk(ctx, cx: 32, cy: 40, h: 36, color: Palette.cobalt)
            drawBambooStalk(ctx, cx: 68, cy: 40, h: 36, color: Palette.jade)
            drawBambooStalk(ctx, cx: 32, cy: 90, h: 36, color: Palette.jade)
            drawBambooStalk(ctx, cx: 68, cy: 90, h: 36, color: Palette.cobalt)
        case 5:
            drawBambooStalk(ctx, cx: 27, cy: 36, h: 34, color: Palette.jade)
            drawBambooStalk(ctx, cx: 73, cy: 36, h: 34, color: Palette.cobalt)
            drawBambooStalk(ctx, cx: 50, cy: 65, h: 34, color: Palette.crimson)
            drawBambooStalk(ctx, cx: 27, cy: 94, h: 34, color: Palette.cobalt)
            drawBambooStalk(ctx, cx: 73, cy: 94, h: 34, color: Palette.jade)
        case 6:
            for y: CGFloat in [40, 90] {
                for x: CGFloat in [26, 50, 74] {
                    drawBambooStalk(ctx, cx: x, cy: y, h: 36, color: Palette.jade)
                }
            }
        case 7:
            drawBambooStalk(ctx, cx: 50, cy: 29, h: 28, color: Palette.crimson)
            for y: CGFloat in [65, 101] {
                drawBambooStalk(ctx, cx: 26, cy: y, h: 28, color: Palette.jade)
                drawBambooStalk(ctx, cx: 50, cy: y, h: 28, color: Palette.cobalt)
                drawBambooStalk(ctx, cx: 74, cy: y, h: 28, color: Palette.jade)
            }
        case 8:
            drawBambooStalk(ctx, cx: 22, cy: 40, h: 34, color: Palette.jade)
            drawBambooStalk(ctx, cx: 41, cy: 42, h: 31, color: Palette.cobalt, angleDeg: 26)
            drawBambooStalk(ctx, cx: 59, cy: 42, h: 31, color: Palette.cobalt, angleDeg: -26)
            drawBambooStalk(ctx, cx: 78, cy: 40, h: 34, color: Palette.jade)
            drawBambooStalk(ctx, cx: 22, cy: 91, h: 34, color: Palette.jade)
            drawBambooStalk(ctx, cx: 41, cy: 89, h: 31, color: Palette.cobalt, angleDeg: -26)
            drawBambooStalk(ctx, cx: 59, cy: 89, h: 31, color: Palette.cobalt, angleDeg: 26)
            drawBambooStalk(ctx, cx: 78, cy: 91, h: 34, color: Palette.jade)
        default:
            for y: CGFloat in [29, 65, 101] {
                drawBambooStalk(ctx, cx: 25, cy: y, h: 27, color: Palette.jade)
                drawBambooStalk(ctx, cx: 50, cy: y, h: 27, color: Palette.crimson)
                drawBambooStalk(ctx, cx: 75, cy: y, h: 27, color: Palette.cobalt)
            }
        }
    } else if tileIndex < 27 {
        // Dot 1..9
        let n = tileIndex - 18 + 1
        switch n {
        case 1:
            drawOnePinMedallion(ctx)
        case 2:
            drawPinCircle(ctx, cx: 50, cy: 40, r: 16, fill: Palette.jade)
            drawPinCircle(ctx, cx: 50, cy: 90, r: 16, fill: Palette.cobalt)
        case 3:
            drawPinCircle(ctx, cx: 27, cy: 33, r: 13.5, fill: Palette.cobalt)
            drawPinCircle(ctx, cx: 50, cy: 65, r: 13.5, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 73, cy: 97, r: 13.5, fill: Palette.jade)
        case 4:
            drawPinCircle(ctx, cx: 31, cy: 40, r: 14, fill: Palette.cobalt)
            drawPinCircle(ctx, cx: 69, cy: 40, r: 14, fill: Palette.jade)
            drawPinCircle(ctx, cx: 31, cy: 90, r: 14, fill: Palette.jade)
            drawPinCircle(ctx, cx: 69, cy: 90, r: 14, fill: Palette.cobalt)
        case 5:
            drawPinCircle(ctx, cx: 27, cy: 34, r: 12.2, fill: Palette.cobalt)
            drawPinCircle(ctx, cx: 73, cy: 34, r: 12.2, fill: Palette.jade)
            drawPinCircle(ctx, cx: 50, cy: 65, r: 12.6, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 27, cy: 96, r: 12.2, fill: Palette.jade)
            drawPinCircle(ctx, cx: 73, cy: 96, r: 12.2, fill: Palette.cobalt)
        case 6:
            drawPinCircle(ctx, cx: 32, cy: 30, r: 12.2, fill: Palette.jade)
            drawPinCircle(ctx, cx: 68, cy: 30, r: 12.2, fill: Palette.jade)
            drawPinCircle(ctx, cx: 32, cy: 65, r: 12.2, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 68, cy: 65, r: 12.2, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 32, cy: 99, r: 12.2, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 68, cy: 99, r: 12.2, fill: Palette.crimson)
        case 7:
            drawPinCircle(ctx, cx: 25, cy: 25, r: 10.6, fill: Palette.jade)
            drawPinCircle(ctx, cx: 50, cy: 35, r: 10.6, fill: Palette.jade)
            drawPinCircle(ctx, cx: 75, cy: 45, r: 10.6, fill: Palette.jade)
            drawPinCircle(ctx, cx: 31, cy: 73, r: 11.4, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 69, cy: 73, r: 11.4, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 31, cy: 102, r: 11.4, fill: Palette.crimson)
            drawPinCircle(ctx, cx: 69, cy: 102, r: 11.4, fill: Palette.crimson)
        case 8:
            for y: CGFloat in [24, 51, 79, 106] {
                drawPinCircle(ctx, cx: 31, cy: y, r: 11, fill: Palette.cobalt)
                drawPinCircle(ctx, cx: 69, cy: y, r: 11, fill: Palette.cobalt)
            }
        default:
            for (y, fill) in [(29.0, Palette.jade), (65.0, Palette.crimson), (101.0, Palette.cobalt)] {
                drawPinCircle(ctx, cx: 24, cy: y, r: 10.8, fill: fill)
                drawPinCircle(ctx, cx: 50, cy: y, r: 10.8, fill: fill)
                drawPinCircle(ctx, cx: 76, cy: y, r: 10.8, fill: fill)
            }
        }
    } else if tileIndex == 33 {
        // White dragon (白板)
        ctx.setStrokeColor(Palette.cobalt)
        ctx.setLineWidth(5)
        addRoundedRect(ctx, rect: CGRect(x: 20, y: 23, width: 60, height: 84), radius: 6)
        ctx.strokePath()

        ctx.setStrokeColor(Palette.ink)
        ctx.setLineWidth(1.8)
        ctx.setLineDash(phase: 0, lengths: [6, 4])
        addRoundedRect(ctx, rect: CGRect(x: 27, y: 30, width: 46, height: 70), radius: 3)
        ctx.strokePath()
        ctx.setLineDash(phase: 0, lengths: [])

        ctx.setStrokeColor(Palette.cobalt)
        ctx.setLineWidth(3)
        ctx.setLineCap(.round)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 20, y: 36)); ctx.addLine(to: CGPoint(x: 29, y: 23))
        ctx.move(to: CGPoint(x: 71, y: 23)); ctx.addLine(to: CGPoint(x: 80, y: 36))
        ctx.move(to: CGPoint(x: 20, y: 94)); ctx.addLine(to: CGPoint(x: 29, y: 107))
        ctx.move(to: CGPoint(x: 71, y: 107)); ctx.addLine(to: CGPoint(x: 80, y: 94))
        ctx.strokePath()
    } else {
        // Winds & Dragons (27..32)
        let glyphs = [27: "東", 28: "南", 29: "西", 30: "北", 31: "中", 32: "發"]
        let color: CGColor = tileIndex == 31 ? Palette.crimson : tileIndex == 32 ? Palette.jade : Palette.ink
        let stroke: CGFloat = (tileIndex == 31 || tileIndex == 32) ? 1.4 : 0.8
        drawCenteredText(ctx, text: glyphs[tileIndex] ?? "?", x: 50, y: 65, fontSize: 58, fill: color, strokeWidth: stroke)
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

    // Piltover deep teal felt base (#14282E)
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

    // Fine canvas tooth wrapped toroidally
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
    guard args.count >= 4 else {
        fputs("Usage: rasterize-assets.swift <atlas-png> <impasto-png> <brush-png>\n", stderr)
        exit(1)
    }

    let atlasUrl = URL(fileURLWithPath: args[1])
    let impastoUrl = URL(fileURLWithPath: args[2])
    let brushUrl = URL(fileURLWithPath: args[3])

    let atlasW = 1152
    let atlasH = 960
    let cellW = 144
    let cellH = 192
    let frameW = 136
    let frameH = 184
    let pad = 4
    let cols = 8

    let atlasCtx = makeContext(width: atlasW, height: atlasH)
    atlasCtx.clear(CGRect(x: 0, y: 0, width: atlasW, height: atlasH))

    for t in 0..<34 {
        let col = t % cols
        let row = t / cols
        let frame = CGRect(x: col * cellW + pad, y: row * cellH + pad, width: frameW, height: frameH)
        drawTileFace(atlasCtx, frame: frame, tileIndex: t)
    }

    let bodyKinds = ["standing", "laid", "back", "shadow", "laid_back"]
    for (idx, kind) in bodyKinds.enumerated() {
        let slot = 34 + idx
        let col = slot % cols
        let row = slot / cols
        let frame = CGRect(x: col * cellW + pad, y: row * cellH + pad, width: frameW, height: frameH)
        drawImpastoBody(atlasCtx, frame: frame, kind: kind)
    }

    try savePNG(ctx: atlasCtx, url: atlasUrl)
    try buildFeltImpasto(url: impastoUrl)
    try buildFeltBrush(url: brushUrl)
}

try main()
