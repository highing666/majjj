import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

// Deterministic @2x 2D comic impasto portrait atlas rasterizer (5 personas x 6 moods = 30 frames, 1536x1280).

struct SeededRNG {
    private var state: UInt64
    init(seed: UInt64) { self.state = seed }
    mutating func next() -> Double {
        state = state &* 6364136223846793005 &+ 1442695040888963407
        let val = (state >> 32) & 0xFFFFFFFF
        return Double(val) / Double(UInt32.max)
    }
}

func rgb(_ hex: UInt64, alpha: CGFloat = 1.0) -> CGColor {
    let r = CGFloat((hex >> 16) & 0xFF) / 255.0
    let g = CGFloat((hex >> 8) & 0xFF) / 255.0
    let b = CGFloat(hex & 0xFF) / 255.0
    return CGColor(srgbRed: r, green: g, blue: b, alpha: alpha)
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
        throw NSError(domain: "PersonaAtlas", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to create CGImage"])
    }
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
        throw NSError(domain: "PersonaAtlas", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to create PNG destination"])
    }
    CGImageDestinationAddImage(dest, image, nil)
    if !CGImageDestinationFinalize(dest) {
        throw NSError(domain: "PersonaAtlas", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to finalize PNG"])
    }
}

func addRoundedRect(_ ctx: CGContext, rect: CGRect, radius: CGFloat) {
    ctx.addPath(CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil))
}

struct PersonaTheme {
    let id: String
    let seed: UInt64
    let bgTop: CGColor
    let bgBottom: CGColor
    let accent: CGColor
    let accentSoft: CGColor
    let skin: CGColor
    let skinHi: CGColor
    let skinShadow: CGColor
    let hair: CGColor
    let hairHi: CGColor
    let outfit: CGColor
    let outfitDark: CGColor
    let trim: CGColor
}

let INK = rgb(0x16181F)
let WHITE = rgb(0xFFFDF8)
let GOLD = rgb(0xFFE074)
let CRIMSON = rgb(0xE83B2E)

let PERSONAS: [PersonaTheme] = [
    PersonaTheme(
        id: "QuickHothead",
        seed: 101,
        bgTop: rgb(0x9E2B1C),
        bgBottom: rgb(0x3C0E0A),
        accent: rgb(0xFF8142),
        accentSoft: rgb(0xFFC278, alpha: 0.35),
        skin: rgb(0xF3BE95),
        skinHi: rgb(0xFFE1C4),
        skinShadow: rgb(0xCA865E),
        hair: rgb(0x281410),
        hairHi: rgb(0x633325),
        outfit: rgb(0xC93927),
        outfitDark: rgb(0x7D1B12),
        trim: rgb(0xF7C95C)
    ),
    PersonaTheme(
        id: "TeaSarcasm",
        seed: 202,
        bgTop: rgb(0x1E6957),
        bgBottom: rgb(0x0B2922),
        accent: rgb(0x5AE2B7),
        accentSoft: rgb(0x9DF5D9, alpha: 0.30),
        skin: rgb(0xF8D9C2),
        skinHi: rgb(0xFFF1E4),
        skinShadow: rgb(0xD3A58A),
        hair: rgb(0x191F28),
        hairHi: rgb(0x3D5266),
        outfit: rgb(0x176B5B),
        outfitDark: rgb(0x0D3E36),
        trim: rgb(0xE8C468)
    ),
    PersonaTheme(
        id: "StubbornBlamer",
        seed: 303,
        bgTop: rgb(0x264D84),
        bgBottom: rgb(0x0E1D38),
        accent: rgb(0x6BA9FF),
        accentSoft: rgb(0xA6CCFF, alpha: 0.30),
        skin: rgb(0xEAB58D),
        skinHi: rgb(0xFAD6B6),
        skinShadow: rgb(0xBD8059),
        hair: rgb(0x1C2029),
        hairHi: rgb(0x475166),
        outfit: rgb(0x283D63),
        outfitDark: rgb(0x16233D),
        trim: rgb(0xEC943C)
    ),
    PersonaTheme(
        id: "ChaosInstigator",
        seed: 404,
        bgTop: rgb(0x8A4F17),
        bgBottom: rgb(0x331B09),
        accent: rgb(0xFFC442),
        accentSoft: rgb(0xFFE08C, alpha: 0.34),
        skin: rgb(0xF5C89C),
        skinHi: rgb(0xFFE6C8),
        skinShadow: rgb(0xCE9565),
        hair: rgb(0x402116),
        hairHi: rgb(0x874C32),
        outfit: rgb(0x6D3285),
        outfitDark: rgb(0x3D184F),
        trim: rgb(0xF8BA3A)
    ),
    PersonaTheme(
        id: "TiltedMaster",
        seed: 505,
        bgTop: rgb(0x751F34),
        bgBottom: rgb(0x2B0913),
        accent: rgb(0xF46E7D),
        accentSoft: rgb(0xFFB3BC, alpha: 0.30),
        skin: rgb(0xE9B591),
        skinHi: rgb(0xFBD6BA),
        skinShadow: rgb(0xBE8460),
        hair: rgb(0x717885),
        hairHi: rgb(0xC2C8D4),
        outfit: rgb(0x731C2B),
        outfitDark: rgb(0x420D17),
        trim: rgb(0xE8BA5B)
    ),
]

let MOODS = ["Smug", "Mocking", "Tilted", "Nervous", "Plotting", "Deadpan"]

func drawImpastoBackdrop(_ ctx: CGContext, theme: PersonaTheme, mood: String) {
    let card = CGRect(x: 4, y: 4, width: 240, height: 240)
    ctx.saveGState()
    addRoundedRect(ctx, rect: card, radius: 42)
    ctx.clip()

    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    let topColor: CGColor = mood == "Tilted" ? rgb(0xB8241C) : (mood == "Plotting" ? rgb(0x241B42) : theme.bgTop)
    let botColor: CGColor = mood == "Tilted" ? rgb(0x470B0B) : (mood == "Plotting" ? rgb(0x0D0A1B) : theme.bgBottom)
    if let grad = CGGradient(colorsSpace: cs, colors: [topColor, botColor] as CFArray, locations: [0.0, 1.0]) {
        ctx.drawLinearGradient(grad, start: CGPoint(x: 20, y: 0), end: CGPoint(x: 228, y: 248), options: [])
    }

    // Painterly impasto brush strokes in background
    var rng = SeededRNG(seed: theme.seed &+ UInt64(MOODS.firstIndex(of: mood) ?? 0) &* 31)
    for _ in 0..<24 {
        let bx = CGFloat(rng.next() * 240.0)
        let by = CGFloat(rng.next() * 240.0)
        let bw = CGFloat(26.0 + rng.next() * 58.0)
        let bh = CGFloat(8.0 + rng.next() * 14.0)
        let angle = CGFloat(-0.55 + (rng.next() - 0.5) * 0.5)
        ctx.saveGState()
        ctx.translateBy(x: bx, y: by)
        ctx.rotate(by: angle)
        ctx.setFillColor(rng.next() > 0.45 ? theme.accentSoft : rgb(0x090C12, alpha: 0.22))
        addRoundedRect(ctx, rect: CGRect(x: -bw * 0.5, y: -bh * 0.5, width: bw, height: bh), radius: bh * 0.4)
        ctx.fillPath()
        ctx.restoreGState()
    }

    // Subtle comic halftone corner dots
    ctx.setFillColor(rgb(0xFFF6DF, alpha: 0.14))
    for row in 0..<5 {
        for col in 0..<5 {
            if row + col < 5 {
                let r = CGFloat(4.2 - Double(row + col) * 0.55)
                ctx.fillEllipse(in: CGRect(x: CGFloat(18 + col * 14) - r, y: CGFloat(18 + row * 14) - r, width: r * 2, height: r * 2))
            }
        }
    }

    // Radial rim glow behind head
    if let halo = CGGradient(
        colorsSpace: cs,
        colors: [theme.accentSoft, rgb(0x000000, alpha: 0.0)] as CFArray,
        locations: [0.0, 1.0]
    ) {
        ctx.drawRadialGradient(halo, startCenter: CGPoint(x: 124, y: 122), startRadius: 12, endCenter: CGPoint(x: 124, y: 122), endRadius: 108, options: [])
    }

    ctx.restoreGState()
}

func drawShouldersAndOutfit(_ ctx: CGContext, theme: PersonaTheme) {
    ctx.saveGState()
    let clipRect = CGRect(x: 4, y: 4, width: 240, height: 240)
    addRoundedRect(ctx, rect: clipRect, radius: 42)
    ctx.clip()

    // Base torso silhouette
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 24, y: 248))
    ctx.addQuadCurve(to: CGPoint(x: 124, y: 174), control: CGPoint(x: 48, y: 180))
    ctx.addQuadCurve(to: CGPoint(x: 224, y: 248), control: CGPoint(x: 200, y: 180))
    ctx.closePath()
    ctx.setFillColor(theme.outfit)
    ctx.fillPath()

    // Impasto shadow side on right shoulder
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 136, y: 176))
    ctx.addQuadCurve(to: CGPoint(x: 224, y: 248), control: CGPoint(x: 202, y: 184))
    ctx.addLine(to: CGPoint(x: 146, y: 248))
    ctx.closePath()
    ctx.setFillColor(theme.outfitDark)
    ctx.fillPath()

    // Bold comic outline on shoulders
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 24, y: 248))
    ctx.addQuadCurve(to: CGPoint(x: 124, y: 174), control: CGPoint(x: 48, y: 180))
    ctx.addQuadCurve(to: CGPoint(x: 224, y: 248), control: CGPoint(x: 200, y: 180))
    ctx.setStrokeColor(INK)
    ctx.setLineWidth(6.0)
    ctx.strokePath()

    // Persona-distinctive collar / trim
    ctx.setStrokeColor(theme.trim)
    ctx.setLineWidth(6.5)
    ctx.setLineCap(.round)
    ctx.setLineJoin(.round)
    switch theme.id {
    case "QuickHothead":
        // Athletic high zip collar + lightning chevron
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 92, y: 182))
        ctx.addLine(to: CGPoint(x: 124, y: 222))
        ctx.addLine(to: CGPoint(x: 156, y: 182))
        ctx.strokePath()
        ctx.setStrokeColor(WHITE)
        ctx.setLineWidth(3.5)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 124, y: 218))
        ctx.addLine(to: CGPoint(x: 124, y: 246))
        ctx.strokePath()
    case "TeaSarcasm":
        // Mandarin collar with gold diagonal closure
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 100, y: 184))
        ctx.addLine(to: CGPoint(x: 148, y: 184))
        ctx.addLine(to: CGPoint(x: 176, y: 236))
        ctx.strokePath()
        // Porcelain teacup rim motif in bottom-left corner
        let cupRect = CGRect(x: 34, y: 208, width: 34, height: 26)
        ctx.setFillColor(WHITE)
        addRoundedRect(ctx, rect: cupRect, radius: 9)
        ctx.fillPath()
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(3.5)
        addRoundedRect(ctx, rect: cupRect, radius: 9)
        ctx.strokePath()
        ctx.setStrokeColor(theme.accent)
        ctx.setLineWidth(2.6)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 44, y: 202))
        ctx.addQuadCurve(to: CGPoint(x: 48, y: 186), control: CGPoint(x: 38, y: 194))
        ctx.move(to: CGPoint(x: 56, y: 202))
        ctx.addQuadCurve(to: CGPoint(x: 60, y: 188), control: CGPoint(x: 50, y: 195))
        ctx.strokePath()
    case "StubbornBlamer":
        // Upturned bomber jacket lapels
        ctx.setFillColor(theme.trim)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 86, y: 186))
        ctx.addLine(to: CGPoint(x: 108, y: 220))
        ctx.addLine(to: CGPoint(x: 118, y: 186))
        ctx.closePath()
        ctx.fillPath()
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 162, y: 186))
        ctx.addLine(to: CGPoint(x: 140, y: 220))
        ctx.addLine(to: CGPoint(x: 130, y: 186))
        ctx.closePath()
        ctx.fillPath()
    case "ChaosInstigator":
        // Bright gold scarf wrap
        ctx.setFillColor(theme.trim)
        addRoundedRect(ctx, rect: CGRect(x: 78, y: 176, width: 92, height: 28), radius: 14)
        ctx.fillPath()
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.5)
        addRoundedRect(ctx, rect: CGRect(x: 78, y: 176, width: 92, height: 28), radius: 14)
        ctx.strokePath()
        // Scarf tail
        ctx.setFillColor(theme.trim)
        addRoundedRect(ctx, rect: CGRect(x: 144, y: 194, width: 22, height: 50), radius: 8)
        ctx.fillPath()
        ctx.setStrokeColor(INK)
        addRoundedRect(ctx, rect: CGRect(x: 144, y: 194, width: 22, height: 50), radius: 8)
        ctx.strokePath()
    default: // TiltedMaster
        // Traditional Tangzhuang brass frog buttons
        ctx.setStrokeColor(theme.trim)
        ctx.setLineWidth(5.5)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 124, y: 186))
        ctx.addLine(to: CGPoint(x: 124, y: 244))
        ctx.strokePath()
        for by in [CGFloat(202), CGFloat(222), CGFloat(240)] {
            ctx.beginPath()
            ctx.move(to: CGPoint(x: 108, y: by))
            ctx.addLine(to: CGPoint(x: 140, y: by))
            ctx.strokePath()
        }
    }

    ctx.restoreGState()
}

func drawHeadAndHair(_ ctx: CGContext, theme: PersonaTheme, mood: String) {
    // Neck
    let neck = CGRect(x: 106, y: 162, width: 36, height: 30)
    ctx.setFillColor(theme.skinShadow)
    addRoundedRect(ctx, rect: neck, radius: 12)
    ctx.fillPath()
    ctx.setStrokeColor(INK)
    ctx.setLineWidth(5)
    addRoundedRect(ctx, rect: neck, radius: 12)
    ctx.strokePath()

    // Ears
    for ex in [CGFloat(62), CGFloat(170)] {
        let ear = CGRect(x: ex, y: 118, width: 16, height: 28)
        ctx.setFillColor(theme.skin)
        ctx.fillEllipse(in: ear)
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.5)
        ctx.strokeEllipse(in: ear)
    }

    // Back hair volume for TeaSarcasm
    if theme.id == "TeaSarcasm" {
        ctx.setFillColor(theme.hair)
        ctx.fillEllipse(in: CGRect(x: 56, y: 56, width: 136, height: 136))
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(5)
        ctx.strokeEllipse(in: CGRect(x: 56, y: 56, width: 136, height: 136))
        // Top bun + gold hairpin
        ctx.setFillColor(theme.hair)
        ctx.fillEllipse(in: CGRect(x: 98, y: 24, width: 52, height: 42))
        ctx.setStrokeColor(INK)
        ctx.strokeEllipse(in: CGRect(x: 98, y: 24, width: 52, height: 42))
        ctx.setStrokeColor(theme.trim)
        ctx.setLineWidth(5)
        ctx.setLineCap(.round)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 82, y: 42))
        ctx.addLine(to: CGPoint(x: 170, y: 36))
        ctx.strokePath()
        ctx.setFillColor(theme.accent)
        ctx.fillEllipse(in: CGRect(x: 164, y: 29, width: 14, height: 14))
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(3)
        ctx.strokeEllipse(in: CGRect(x: 164, y: 29, width: 14, height: 14))
    }

    // Face shape (slightly tailored per character)
    let faceRect: CGRect = theme.id == "StubbornBlamer"
        ? CGRect(x: 70, y: 68, width: 108, height: 116)
        : CGRect(x: 72, y: 68, width: 104, height: 116)
    let faceRadius: CGFloat = theme.id == "StubbornBlamer" ? 38 : 48

    ctx.saveGState()
    addRoundedRect(ctx, rect: faceRect, radius: faceRadius)
    ctx.clip()

    ctx.setFillColor(theme.skin)
    ctx.fill(faceRect)

    // Impasto warm highlight left cheek/forehead
    ctx.setFillColor(theme.skinHi)
    ctx.fillEllipse(in: CGRect(x: 76, y: 74, width: 64, height: 74))

    // Impasto shadow plane right side of face
    ctx.setFillColor(theme.skinShadow)
    ctx.setAlpha(0.55)
    ctx.fillEllipse(in: CGRect(x: 138, y: 76, width: 54, height: 112))
    ctx.setAlpha(1.0)

    // Mood skin overlays (red flush for Tilted, shadow veil for Plotting, pale gloom for Nervous/Deadpan)
    if mood == "Tilted" {
        let cs = CGColorSpace(name: CGColorSpace.sRGB)!
        if let flush = CGGradient(
            colorsSpace: cs,
            colors: [rgb(0xE83226, alpha: 0.52), rgb(0xE83226, alpha: 0.08)] as CFArray,
            locations: [0.0, 1.0]
        ) {
            ctx.drawLinearGradient(flush, start: CGPoint(x: 124, y: 70), end: CGPoint(x: 124, y: 184), options: [])
        }
    } else if mood == "Plotting" {
        let cs = CGColorSpace(name: CGColorSpace.sRGB)!
        if let veil = CGGradient(
            colorsSpace: cs,
            colors: [rgb(0x0E1224, alpha: 0.72), rgb(0x0E1224, alpha: 0.0)] as CFArray,
            locations: [0.0, 1.0]
        ) {
            ctx.drawLinearGradient(veil, start: CGPoint(x: 124, y: 74), end: CGPoint(x: 124, y: 146), options: [])
        }
    } else if mood == "Nervous" {
        let cs = CGColorSpace(name: CGColorSpace.sRGB)!
        if let pale = CGGradient(
            colorsSpace: cs,
            colors: [rgb(0x46689E, alpha: 0.34), rgb(0x46689E, alpha: 0.0)] as CFArray,
            locations: [0.0, 1.0]
        ) {
            ctx.drawLinearGradient(pale, start: CGPoint(x: 124, y: 70), end: CGPoint(x: 124, y: 132), options: [])
        }
    }
    ctx.restoreGState()

    // Face ink outline
    ctx.setStrokeColor(INK)
    ctx.setLineWidth(5.5)
    addRoundedRect(ctx, rect: faceRect, radius: faceRadius)
    ctx.strokePath()

    // Persona-specific cheek detail (bandage for StubbornBlamer, distinguished mustache for TiltedMaster)
    if theme.id == "StubbornBlamer" {
        ctx.saveGState()
        ctx.translateBy(x: 152, y: 146)
        ctx.rotate(by: -0.22)
        let plaster = CGRect(x: -14, y: -6, width: 28, height: 12)
        ctx.setFillColor(rgb(0xF6DFB8))
        addRoundedRect(ctx, rect: plaster, radius: 4)
        ctx.fillPath()
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(2.6)
        addRoundedRect(ctx, rect: plaster, radius: 4)
        ctx.strokePath()
        ctx.restoreGState()
    } else if theme.id == "TiltedMaster" {
        // Silver-gray mustache above mouth
        ctx.setFillColor(theme.hair)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 124, y: 147))
        ctx.addQuadCurve(to: CGPoint(x: 96, y: 158), control: CGPoint(x: 106, y: 144))
        ctx.addQuadCurve(to: CGPoint(x: 124, y: 155), control: CGPoint(x: 112, y: 162))
        ctx.addQuadCurve(to: CGPoint(x: 152, y: 158), control: CGPoint(x: 136, y: 162))
        ctx.addQuadCurve(to: CGPoint(x: 124, y: 147), control: CGPoint(x: 142, y: 144))
        ctx.closePath()
        ctx.fillPath()
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(2.8)
        ctx.strokePath()
    }

    // Front hairstyle per persona
    ctx.setFillColor(theme.hair)
    ctx.setStrokeColor(INK)
    ctx.setLineWidth(5.0)
    ctx.setLineJoin(.round)

    switch theme.id {
    case "QuickHothead":
        // Spiky dynamic flame hair + red headband
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 68, y: 114))
        ctx.addLine(to: CGPoint(x: 56, y: 78))
        ctx.addLine(to: CGPoint(x: 84, y: 66))
        ctx.addLine(to: CGPoint(x: 94, y: 34))
        ctx.addLine(to: CGPoint(x: 118, y: 54))
        ctx.addLine(to: CGPoint(x: 138, y: 28))
        ctx.addLine(to: CGPoint(x: 150, y: 56))
        ctx.addLine(to: CGPoint(x: 184, y: 50))
        ctx.addLine(to: CGPoint(x: 174, y: 84))
        ctx.addLine(to: CGPoint(x: 184, y: 108))
        ctx.addQuadCurve(to: CGPoint(x: 68, y: 114), control: CGPoint(x: 124, y: 76))
        ctx.closePath()
        ctx.fillPath()
        ctx.strokePath()

        // Signature fiery headband
        let band = CGRect(x: 68, y: 82, width: 112, height: 16)
        ctx.setFillColor(CRIMSON)
        addRoundedRect(ctx, rect: band, radius: 6)
        ctx.fillPath()
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(3.8)
        addRoundedRect(ctx, rect: band, radius: 6)
        ctx.strokePath()
    case "TeaSarcasm":
        // Sleek side-swept bangs
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 68, y: 116))
        ctx.addQuadCurve(to: CGPoint(x: 124, y: 54), control: CGPoint(x: 74, y: 62))
        ctx.addQuadCurve(to: CGPoint(x: 180, y: 116), control: CGPoint(x: 174, y: 62))
        ctx.addQuadCurve(to: CGPoint(x: 136, y: 82), control: CGPoint(x: 162, y: 90))
        ctx.addQuadCurve(to: CGPoint(x: 68, y: 116), control: CGPoint(x: 102, y: 98))
        ctx.closePath()
        ctx.fillPath()
        ctx.strokePath()
    case "StubbornBlamer":
        // Bold square pompadour quiff
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 68, y: 110))
        ctx.addLine(to: CGPoint(x: 64, y: 62))
        ctx.addQuadCurve(to: CGPoint(x: 132, y: 36), control: CGPoint(x: 84, y: 38))
        ctx.addQuadCurve(to: CGPoint(x: 186, y: 72), control: CGPoint(x: 182, y: 42))
        ctx.addLine(to: CGPoint(x: 178, y: 110))
        ctx.addQuadCurve(to: CGPoint(x: 68, y: 110), control: CGPoint(x: 124, y: 78))
        ctx.closePath()
        ctx.fillPath()
        ctx.strokePath()
    case "ChaosInstigator":
        // Messy hair + jaunty tilted cap
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 66, y: 116))
        ctx.addLine(to: CGPoint(x: 78, y: 76))
        ctx.addLine(to: CGPoint(x: 170, y: 76))
        ctx.addLine(to: CGPoint(x: 182, y: 116))
        ctx.addLine(to: CGPoint(x: 162, y: 94))
        ctx.addLine(to: CGPoint(x: 134, y: 104))
        ctx.addLine(to: CGPoint(x: 118, y: 90))
        ctx.addLine(to: CGPoint(x: 92, y: 106))
        ctx.closePath()
        ctx.fillPath()
        ctx.strokePath()

        // Tilted visor cap
        ctx.saveGState()
        ctx.translateBy(x: 124, y: 64)
        ctx.rotate(by: -0.14)
        ctx.setFillColor(theme.trim)
        ctx.fillEllipse(in: CGRect(x: -56, y: -22, width: 112, height: 42))
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.5)
        ctx.strokeEllipse(in: CGRect(x: -56, y: -22, width: 112, height: 42))
        // Cap brim
        ctx.setFillColor(theme.outfit)
        addRoundedRect(ctx, rect: CGRect(x: -44, y: 4, width: 106, height: 15), radius: 7)
        ctx.fillPath()
        ctx.strokePath()
        ctx.restoreGState()
    default: // TiltedMaster
        // Swept-back silver-streaked hair
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 68, y: 116))
        ctx.addQuadCurve(to: CGPoint(x: 84, y: 48), control: CGPoint(x: 62, y: 68))
        ctx.addQuadCurve(to: CGPoint(x: 168, y: 50), control: CGPoint(x: 126, y: 34))
        ctx.addQuadCurve(to: CGPoint(x: 180, y: 116), control: CGPoint(x: 188, y: 72))
        ctx.addQuadCurve(to: CGPoint(x: 124, y: 78), control: CGPoint(x: 160, y: 84))
        ctx.addQuadCurve(to: CGPoint(x: 68, y: 116), control: CGPoint(x: 88, y: 84))
        ctx.closePath()
        ctx.fillPath()
        ctx.strokePath()
    }

    // Impasto hair highlight streak
    ctx.setStrokeColor(theme.hairHi)
    ctx.setLineWidth(4.5)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 92, y: 66))
    ctx.addQuadCurve(to: CGPoint(x: 146, y: 62), control: CGPoint(x: 118, y: 56))
    ctx.strokePath()
}

func drawExpressionAndEffects(_ ctx: CGContext, theme: PersonaTheme, mood: String) {
    // Nose accent
    ctx.setStrokeColor(INK)
    ctx.setLineWidth(3.2)
    ctx.setLineCap(.round)
    ctx.beginPath()
    ctx.move(to: CGPoint(x: 124, y: 132))
    ctx.addLine(to: CGPoint(x: 121, y: 142))
    ctx.addLine(to: CGPoint(x: 127, y: 143))
    ctx.strokePath()

    let mouthY: CGFloat = theme.id == "TiltedMaster" ? 166 : 162

    switch mood {
    case "Smug":
        // Cocky brows
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(5.5)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 84, y: 112))
        ctx.addQuadCurve(to: CGPoint(x: 114, y: 112), control: CGPoint(x: 99, y: 103))
        ctx.move(to: CGPoint(x: 134, y: 108))
        ctx.addQuadCurve(to: CGPoint(x: 166, y: 104), control: CGPoint(x: 150, y: 94))
        ctx.strokePath()

        // Confident half-smirk eyes
        for cx in [CGFloat(99), CGFloat(149)] {
            ctx.setFillColor(WHITE)
            ctx.fillEllipse(in: CGRect(x: cx - 11, y: 120, width: 22, height: 15))
            ctx.setStrokeColor(INK)
            ctx.setLineWidth(3.4)
            ctx.strokeEllipse(in: CGRect(x: cx - 11, y: 120, width: 22, height: 15))
            ctx.setFillColor(INK)
            ctx.fillEllipse(in: CGRect(x: cx - 5, y: 123, width: 10, height: 10))
            ctx.setFillColor(WHITE)
            ctx.fillEllipse(in: CGRect(x: cx - 3, y: 124, width: 4, height: 4))
        }

        // Lopsided toothy grin
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.5)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 103, y: mouthY + 2))
        ctx.addQuadCurve(to: CGPoint(x: 152, y: mouthY - 8), control: CGPoint(x: 130, y: mouthY + 12))
        ctx.strokePath()
        ctx.setFillColor(WHITE)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 128, y: mouthY + 1))
        ctx.addLine(to: CGPoint(x: 144, y: mouthY - 4))
        ctx.addLine(to: CGPoint(x: 139, y: mouthY + 6))
        ctx.closePath()
        ctx.fillPath()

        // Golden comic sparkle burst top-right
        drawSparkle(ctx, cx: 204, cy: 58, r: 22, fill: GOLD)
        drawSparkle(ctx, cx: 46, cy: 66, r: 12, fill: WHITE)

    case "Mocking":
        // Arched tea-art brows
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(5.0)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 84, y: 112))
        ctx.addQuadCurve(to: CGPoint(x: 114, y: 115), control: CGPoint(x: 99, y: 107))
        ctx.move(to: CGPoint(x: 134, y: 112))
        ctx.addQuadCurve(to: CGPoint(x: 166, y: 107), control: CGPoint(x: 150, y: 98))
        ctx.strokePath()

        // Half-lidded side-eye glance
        for cx in [CGFloat(99), CGFloat(149)] {
            ctx.setFillColor(WHITE)
            ctx.fill(CGRect(x: cx - 12, y: 125, width: 24, height: 10))
            ctx.setFillColor(INK)
            ctx.fillEllipse(in: CGRect(x: cx + 1, y: 125, width: 9, height: 9))
            ctx.setStrokeColor(INK)
            ctx.setLineWidth(4.5)
            ctx.beginPath()
            ctx.move(to: CGPoint(x: cx - 13, y: 125))
            ctx.addLine(to: CGPoint(x: cx + 13, y: 125))
            ctx.strokePath()
        }

        // Cool curled smirk
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.2)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 106, y: mouthY + 2))
        ctx.addQuadCurve(to: CGPoint(x: 147, y: mouthY - 6), control: CGPoint(x: 130, y: mouthY + 4))
        ctx.strokePath()

        // Sneer comic glint marks
        ctx.setStrokeColor(theme.accent)
        ctx.setLineWidth(4)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 192, y: 44))
        ctx.addLine(to: CGPoint(x: 214, y: 52))
        ctx.move(to: CGPoint(x: 196, y: 62))
        ctx.addLine(to: CGPoint(x: 218, y: 66))
        ctx.strokePath()

    case "Tilted":
        // Steep furious V brows
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(6.5)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 82, y: 104))
        ctx.addLine(to: CGPoint(x: 116, y: 120))
        ctx.move(to: CGPoint(x: 166, y: 104))
        ctx.addLine(to: CGPoint(x: 132, y: 120))
        ctx.strokePath()

        // Bulging fiery eyes with tiny enraged pupils
        for cx in [CGFloat(99), CGFloat(149)] {
            ctx.setFillColor(WHITE)
            ctx.fillEllipse(in: CGRect(x: cx - 11, y: 118, width: 22, height: 19))
            ctx.setStrokeColor(INK)
            ctx.setLineWidth(3.8)
            ctx.strokeEllipse(in: CGRect(x: cx - 11, y: 118, width: 22, height: 19))
            ctx.setFillColor(INK)
            ctx.fillEllipse(in: CGRect(x: cx - 3.5, y: 124, width: 7, height: 7))
        }

        // Shouting clenched-teeth comic mouth
        let shoutRect = CGRect(x: 98, y: mouthY - 8, width: 52, height: 22)
        ctx.setFillColor(rgb(0x7A1216))
        addRoundedRect(ctx, rect: shoutRect, radius: 9)
        ctx.fillPath()
        ctx.setFillColor(WHITE)
        ctx.fill(CGRect(x: 102, y: mouthY - 6, width: 44, height: 6))
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.0)
        addRoundedRect(ctx, rect: shoutRect, radius: 9)
        ctx.strokePath()

        // Iconic popping comic anger vein (💢) + steam puffs
        drawAngerVein(ctx, cx: 198, cy: 56)
        ctx.setFillColor(rgb(0xFFF6F2, alpha: 0.85))
        ctx.fillEllipse(in: CGRect(x: 34, y: 42, width: 26, height: 20))
        ctx.fillEllipse(in: CGRect(x: 48, y: 34, width: 20, height: 18))

    case "Nervous":
        // Worried upturned inner brows
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.8)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 82, y: 116))
        ctx.addQuadCurve(to: CGPoint(x: 114, y: 106), control: CGPoint(x: 98, y: 106))
        ctx.move(to: CGPoint(x: 166, y: 116))
        ctx.addQuadCurve(to: CGPoint(x: 134, y: 106), control: CGPoint(x: 150, y: 106))
        ctx.strokePath()

        // Wide anxious eyes with shrunk pupils
        for cx in [CGFloat(99), CGFloat(149)] {
            ctx.setFillColor(WHITE)
            ctx.fillEllipse(in: CGRect(x: cx - 12, y: 117, width: 24, height: 22))
            ctx.setStrokeColor(INK)
            ctx.setLineWidth(3.5)
            ctx.strokeEllipse(in: CGRect(x: cx - 12, y: 117, width: 24, height: 22))
            ctx.setFillColor(INK)
            ctx.fillEllipse(in: CGRect(x: cx - 3, y: 125, width: 6, height: 6))
        }

        // Wavy tense mouth
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.2)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 102, y: mouthY + 2))
        ctx.addQuadCurve(to: CGPoint(x: 117, y: mouthY + 2), control: CGPoint(x: 109, y: mouthY - 5))
        ctx.addQuadCurve(to: CGPoint(x: 131, y: mouthY + 2), control: CGPoint(x: 124, y: mouthY + 9))
        ctx.addQuadCurve(to: CGPoint(x: 146, y: mouthY + 2), control: CGPoint(x: 139, y: mouthY - 5))
        ctx.strokePath()

        // Glossy comic sweat drop (💧)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 198, y: 42))
        ctx.addQuadCurve(to: CGPoint(x: 214, y: 74), control: CGPoint(x: 216, y: 60))
        ctx.addQuadCurve(to: CGPoint(x: 198, y: 88), control: CGPoint(x: 210, y: 88))
        ctx.addQuadCurve(to: CGPoint(x: 182, y: 74), control: CGPoint(x: 186, y: 88))
        ctx.addQuadCurve(to: CGPoint(x: 198, y: 42), control: CGPoint(x: 180, y: 60))
        ctx.closePath()
        ctx.setFillColor(rgb(0x69C8FF))
        ctx.fillPath()
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(3.5)
        ctx.strokePath()
        ctx.setFillColor(WHITE)
        ctx.fillEllipse(in: CGRect(x: 191, y: 66, width: 6, height: 10))

    case "Plotting":
        // Lowered scheming brows
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(5.5)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 84, y: 112))
        ctx.addLine(to: CGPoint(x: 116, y: 119))
        ctx.move(to: CGPoint(x: 164, y: 112))
        ctx.addLine(to: CGPoint(x: 132, y: 119))
        ctx.strokePath()

        // Glowing narrowed scheming eyes peering out of brow shadow
        for (cx, dir) in [(CGFloat(99), CGFloat(1)), (CGFloat(149), CGFloat(-1))] {
            ctx.beginPath()
            ctx.move(to: CGPoint(x: cx - 12 * dir, y: 124))
            ctx.addLine(to: CGPoint(x: cx + 12 * dir, y: 128))
            ctx.addLine(to: CGPoint(x: cx + 8 * dir, y: 135))
            ctx.addLine(to: CGPoint(x: cx - 10 * dir, y: 133))
            ctx.closePath()
            ctx.setFillColor(GOLD)
            ctx.fillPath()
            ctx.setStrokeColor(INK)
            ctx.setLineWidth(3.0)
            ctx.strokePath()
            ctx.setFillColor(INK)
            ctx.fillEllipse(in: CGRect(x: cx - 2.5, y: 127, width: 5, height: 5))
        }

        // Wide mischievous Cheshire grin
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.5)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 98, y: mouthY - 2))
        ctx.addQuadCurve(to: CGPoint(x: 150, y: mouthY - 2), control: CGPoint(x: 124, y: mouthY + 18))
        ctx.strokePath()

        // Purple plotting aura sparks
        drawSparkle(ctx, cx: 46, cy: 52, r: 14, fill: rgb(0xC69BFF))
        drawSparkle(ctx, cx: 204, cy: 62, r: 16, fill: GOLD)

    default: // Deadpan
        // Straight unimpressed brows + flat horizontal eyelids
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.8)
        for cx in [CGFloat(99), CGFloat(149)] {
            ctx.beginPath()
            ctx.move(to: CGPoint(x: cx - 13, y: 114))
            ctx.addLine(to: CGPoint(x: cx + 13, y: 114))
            ctx.move(to: CGPoint(x: cx - 12, y: 129))
            ctx.addLine(to: CGPoint(x: cx + 12, y: 129))
            ctx.strokePath()
            ctx.setFillColor(INK)
            ctx.fillEllipse(in: CGRect(x: cx - 3.5, y: 129, width: 7, height: 5))
        }

        // Flat deadpan mouth
        ctx.setStrokeColor(INK)
        ctx.setLineWidth(4.2)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 110, y: mouthY + 2))
        ctx.addLine(to: CGPoint(x: 138, y: mouthY + 2))
        ctx.strokePath()

        // Three classic comic speechless/gloom lines (|||)
        ctx.setStrokeColor(rgb(0x9CB5C9))
        ctx.setLineWidth(4.5)
        for (lx, topY, botY) in [(CGFloat(186), CGFloat(44), CGFloat(84)), (CGFloat(198), CGFloat(38), CGFloat(90)), (CGFloat(210), CGFloat(48), CGFloat(82))] {
            ctx.beginPath()
            ctx.move(to: CGPoint(x: lx, y: topY))
            ctx.addLine(to: CGPoint(x: lx, y: botY))
            ctx.strokePath()
        }
    }

    // Persona glasses overlay (drawn cleanly over eyes for TeaSarcasm and TiltedMaster)
    if theme.id == "TeaSarcasm" || theme.id == "TiltedMaster" {
        let rimColor: CGColor = theme.id == "TeaSarcasm" ? rgb(0xE8C468) : rgb(0x3A241A)
        let lensFill: CGColor = mood == "Plotting" ? rgb(0xFFF6D6, alpha: 0.42) : rgb(0xFFFFFF, alpha: 0.14)
        for cx in [CGFloat(99), CGFloat(149)] {
            let lens = CGRect(x: cx - 17, y: 116, width: 34, height: 22)
            ctx.setFillColor(lensFill)
            addRoundedRect(ctx, rect: lens, radius: 7)
            ctx.fillPath()
            ctx.setStrokeColor(rimColor)
            ctx.setLineWidth(3.4)
            addRoundedRect(ctx, rect: lens, radius: 7)
            ctx.strokePath()
        }
        ctx.setStrokeColor(rimColor)
        ctx.setLineWidth(3.2)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: 116, y: 125))
        ctx.addLine(to: CGPoint(x: 132, y: 125))
        ctx.strokePath()
    }
}

func drawSparkle(_ ctx: CGContext, cx: CGFloat, cy: CGFloat, r: CGFloat, fill: CGColor) {
    let inner = r * 0.28
    ctx.beginPath()
    ctx.move(to: CGPoint(x: cx, y: cy - r))
    ctx.addLine(to: CGPoint(x: cx + inner, y: cy - inner))
    ctx.addLine(to: CGPoint(x: cx + r, y: cy))
    ctx.addLine(to: CGPoint(x: cx + inner, y: cy + inner))
    ctx.addLine(to: CGPoint(x: cx, y: cy + r))
    ctx.addLine(to: CGPoint(x: cx - inner, y: cy + inner))
    ctx.addLine(to: CGPoint(x: cx - r, y: cy))
    ctx.addLine(to: CGPoint(x: cx - inner, y: cy - inner))
    ctx.closePath()
    ctx.setFillColor(fill)
    ctx.fillPath()
    ctx.setStrokeColor(INK)
    ctx.setLineWidth(2.6)
    ctx.strokePath()
}

func drawAngerVein(_ ctx: CGContext, cx: CGFloat, cy: CGFloat) {
    ctx.setStrokeColor(CRIMSON)
    ctx.setLineWidth(5.2)
    ctx.setLineCap(.round)
    let dirs: [(CGFloat, CGFloat)] = [(-1, -1), (1, -1), (1, 1), (-1, 1)]
    for (dx, dy) in dirs {
        ctx.beginPath()
        ctx.move(to: CGPoint(x: cx + dx * 16, y: cy + dy * 4))
        ctx.addQuadCurve(
            to: CGPoint(x: cx + dx * 4, y: cy + dy * 16),
            control: CGPoint(x: cx + dx * 4, y: cy + dy * 4)
        )
        ctx.strokePath()
    }
}

func drawPortrait(_ ctx: CGContext, frame: CGRect, theme: PersonaTheme, mood: String) {
    ctx.saveGState()
    ctx.translateBy(x: frame.minX, y: frame.minY)
    ctx.scaleBy(x: frame.width / 248.0, y: frame.height / 248.0)

    drawImpastoBackdrop(ctx, theme: theme, mood: mood)
    drawShouldersAndOutfit(ctx, theme: theme)
    drawHeadAndHair(ctx, theme: theme, mood: mood)
    drawExpressionAndEffects(ctx, theme: theme, mood: mood)

    // Outer comic frame border + inner warm accent rim
    let outer = CGRect(x: 4, y: 4, width: 240, height: 240)
    ctx.setStrokeColor(theme.accent)
    ctx.setAlpha(0.85)
    ctx.setLineWidth(4.5)
    addRoundedRect(ctx, rect: outer.insetBy(dx: 5, dy: 5), radius: 37)
    ctx.strokePath()
    ctx.setAlpha(1.0)

    ctx.setStrokeColor(INK)
    ctx.setLineWidth(6.5)
    addRoundedRect(ctx, rect: outer, radius: 42)
    ctx.strokePath()

    ctx.restoreGState()
}

func main() throws {
    let args = CommandLine.arguments
    guard args.count >= 2 else {
        fputs("Usage: rasterize-personas.swift <output-png>\n", stderr)
        exit(1)
    }

    let outUrl = URL(fileURLWithPath: args[1])
    let cols = MOODS.count
    let rows = PERSONAS.count
    let cellW = 256
    let cellH = 256
    let frameW = 248
    let frameH = 248
    let pad = 4

    let atlasW = cols * cellW // 1536 <= 2048
    let atlasH = rows * cellH // 1280 <= 2048
    let ctx = makeContext(width: atlasW, height: atlasH)
    ctx.clear(CGRect(x: 0, y: 0, width: atlasW, height: atlasH))

    for (row, persona) in PERSONAS.enumerated() {
        for (col, mood) in MOODS.enumerated() {
            let rect = CGRect(
                x: col * cellW + pad,
                y: row * cellH + pad,
                width: frameW,
                height: frameH
            )
            drawPortrait(ctx, frame: rect, theme: persona, mood: mood)
        }
    }

    try savePNG(ctx: ctx, url: outUrl)
}

try main()
