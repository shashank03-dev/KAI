// Lift the cobalt ribbon mark off its light tile into a transparent PNG.
//
// Pipeline (calibrated against kai-app-v3.png, 1254x1254):
//   blueness = B - max(R,G); ratio = blueness / B; darkness = 1 - max(R,G,B)/255
//   keep     = connected components of blueness > 40 that contain pixels of
//              blueness > 90 (the ribbon; drops the tile rim and speckle)
//   holes    = components of ~erode(keep,3) not touching the border:
//              * under 15000 px -> specular gloss inside the ribbon, kept
//                fully opaque with the sampled colour
//              * the rest (the 57k px centre opening) -> transparent
//   alpha    = blue ramp of blueness, chroma-ratio suppression (the tile's
//              blue-grey shadow is desaturated), or darkness for the ribbon's
//              dark crevices (which are dark but only partly blue)
//   colour   = unpremultiplied against the white tile where the pixel is
//              bright enough for that model, so edges stay cobalt on dark
//              backgrounds instead of turning into a white halo
//
// usage: extract_mark <src.png> <dst.png> [pad]
import CoreGraphics
import ImageIO
import Foundation

let args = CommandLine.arguments
guard args.count >= 3 else { fatalError("usage: extract_mark <src> <dst> [pad]") }
let srcPath = args[1], dstPath = args[2]
let pad = args.count > 3 ? (Int(args[3]) ?? 8) : 8

func slog(_ s: String) {
    FileHandle.standardError.write((s + "\n").data(using: .utf8)!)
}

guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: srcPath) as CFURL, nil),
      let cg = CGImageSourceCreateImageAtIndex(source, 0, nil) else { fatalError("cannot load \(srcPath)") }
let w = cg.width, h = cg.height
let n = w * h

var buf = [UInt8](repeating: 0, count: n * 4)
let space = CGColorSpace(name: CGColorSpace.sRGB)!
let info = CGImageAlphaInfo.premultipliedLast.rawValue
guard let ctx = CGContext(data: &buf, width: w, height: h, bitsPerComponent: 8,
                          bytesPerRow: w * 4, space: space, bitmapInfo: info) else { fatalError("ctx") }
ctx.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))

// Unpremultiply so colours are true on the opaque tile.
for i in stride(from: 0, to: buf.count, by: 4) {
    let a = Int(buf[i + 3])
    if a > 0 && a < 255 {
        for c in 0..<3 {
            buf[i + c] = UInt8(min(255, Int(buf[i + c]) * 255 / a))
        }
    }
}

var blueness = [Double](repeating: 0, count: n)
var ratio = [Double](repeating: 0, count: n)
var darkness = [Double](repeating: 0, count: n)
for i in 0..<n {
    let p = i * 4
    let r = Double(buf[p]), g = Double(buf[p + 1]), b = Double(buf[p + 2])
    let bl = b - max(r, g)
    blueness[i] = bl
    ratio[i] = b > 1 ? bl / b : 0
    darkness[i] = 1 - max(r, g, b) / 255
}

// mask0 / core, then connected components that contain core pixels.
var mask0 = [UInt8](repeating: 0, count: n)
var core = [UInt8](repeating: 0, count: n)
for i in 0..<n {
    if blueness[i] > 40 { mask0[i] = 1 }
    if blueness[i] > 90 { core[i] = 1 }
}
var labels = [Int32](repeating: -1, count: n)
var stack = [Int]()
var nextLabel: Int32 = 0
for start in 0..<n where mask0[start] == 1 && labels[start] == -1 {
    let label = nextLabel
    nextLabel += 1
    stack.removeAll(keepingCapacity: true)
    stack.append(start)
    labels[start] = label
    while let idx = stack.popLast() {
        let x = idx % w, y = idx / w
        if x > 0     && mask0[idx - 1] == 1 && labels[idx - 1] == -1 { labels[idx - 1] = label; stack.append(idx - 1) }
        if x < w - 1 && mask0[idx + 1] == 1 && labels[idx + 1] == -1 { labels[idx + 1] = label; stack.append(idx + 1) }
        if y > 0     && mask0[idx - w] == 1 && labels[idx - w] == -1 { labels[idx - w] = label; stack.append(idx - w) }
        if y < h - 1 && mask0[idx + w] == 1 && labels[idx + w] == -1 { labels[idx + w] = label; stack.append(idx + w) }
    }
}
var coreCount = [Int](repeating: 0, count: Int(nextLabel))
for i in 0..<n where core[i] == 1 && labels[i] >= 0 {
    coreCount[Int(labels[i])] += 1
}
var keep = [UInt8](repeating: 0, count: n)
for i in 0..<n where labels[i] >= 0 && coreCount[Int(labels[i])] >= 1000 {
    keep[i] = 1
}

// Separable box dilation (1s inside radius).
func dilate(_ mask: [UInt8], radius: Int) -> [UInt8] {
    var src = mask
    var out = [UInt8](repeating: 0, count: n)
    var row = [UInt8](repeating: 0, count: max(w, h))
    for y in 0..<h {
        var count = 0
        for x in 0..<min(radius + 1, w) where src[y * w + x] == 1 { count += 1 }
        for x in 0..<w {
            if x + radius < w && src[y * w + x + radius] == 1 { count += 1 }
            if x - radius - 1 >= 0 && src[y * w + x - radius - 1] == 1 { count -= 1 }
            row[x] = count > 0 ? 1 : 0
        }
        for x in 0..<w { out[y * w + x] = row[x] }
    }
    src = out
    for x in 0..<w {
        var count = 0
        for y in 0..<min(radius + 1, h) where src[y * w + x] == 1 { count += 1 }
        for y in 0..<h {
            if y + radius < h && src[(y + radius) * w + x] == 1 { count += 1 }
            if y - radius - 1 >= 0 && src[(y - radius - 1) * w + x] == 1 { count -= 1 }
            row[y] = count > 0 ? 1 : 0
        }
        for y in 0..<h { out[y * w + x] = row[y] }
    }
    return out
}

// Direct erosion by a square structuring element.
func erode(_ m: [UInt8], radius: Int) -> [UInt8] {
    var out = [UInt8](repeating: 0, count: n)
    for y in 0..<h {
        for x in 0..<w where m[y * w + x] == 1 {
            var solid = true
            outer: for dy in -radius...radius {
                let yy = y + dy
                if yy < 0 || yy >= h { solid = false; break outer }
                for dx in -radius...radius {
                    let xx = x + dx
                    if xx < 0 || xx >= w || m[yy * w + xx] == 0 { solid = false; break outer }
                }
            }
            if solid { out[y * w + x] = 1 }
        }
    }
    return out
}

// Opening with radius 2: drops hairline residue (tile edge/seam) but leaves
// the ribbon (everywhere thicker than 50 px) untouched.
let opened = dilate(erode(keep, radius: 2), radius: 2)
for i in 0..<n { keep[i] = keep[i] == 1 && opened[i] == 1 ? 1 : 0 }

// Erode `keep` by 3 so the gloss hole components extend into the blend ring.
var keepEroded = erode(keep, radius: 3)

// Interior components of ~keepEroded that do not touch the border.
var holeLabels = [Int32](repeating: -1, count: n)
var holeSizes: [Int] = []
var holeBorder: [Bool] = []
var holeStack = [Int]()
for start in 0..<n where keepEroded[start] == 0 && holeLabels[start] == -1 {
    let label = Int32(holeSizes.count)
    var size = 0
    var touchesBorder = false
    holeStack.removeAll(keepingCapacity: true)
    holeStack.append(start)
    holeLabels[start] = label
    while let idx = holeStack.popLast() {
        size += 1
        let x = idx % w, y = idx / w
        if x == 0 || y == 0 || x == w - 1 || y == h - 1 { touchesBorder = true }
        if x > 0     && keepEroded[idx - 1] == 0 && holeLabels[idx - 1] == -1 { holeLabels[idx - 1] = label; holeStack.append(idx - 1) }
        if x < w - 1 && keepEroded[idx + 1] == 0 && holeLabels[idx + 1] == -1 { holeLabels[idx + 1] = label; holeStack.append(idx + 1) }
        if y > 0     && keepEroded[idx - w] == 0 && holeLabels[idx - w] == -1 { holeLabels[idx - w] = label; holeStack.append(idx - w) }
        if y < h - 1 && keepEroded[idx + w] == 0 && holeLabels[idx + w] == -1 { holeLabels[idx + w] = label; holeStack.append(idx + w) }
    }
    holeSizes.append(size)
    holeBorder.append(touchesBorder)
}
let glossMax = 15000
var gloss = [UInt8](repeating: 0, count: n)
var centerHoles = 0
for i in 0..<n where holeLabels[i] >= 0 {
    let l = Int(holeLabels[i])
    if !holeBorder[l] {
        if holeSizes[l] < glossMax { gloss[i] = 1 } else { centerHoles += 1 }
    }
}
slog("components \(nextLabel); holes \(holeSizes.count); gloss px \(gloss.reduce(0) { $0 + Int($1) })")

func smoothstep(_ x: Double, _ a: Double, _ b: Double) -> Double {
    let t = max(0, min(1, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)
}

// Final alpha and colour.
var alpha = [Double](repeating: 0, count: n)
var minX = w, minY = h, maxX = -1, maxY = -1
for y in 0..<h {
    for x in 0..<w {
        let i = y * w + x
        var a = 0.0
        if gloss[i] == 1 {
            a = 1.0
        } else if keep[i] == 1 {
            let blueEdge = smoothstep(blueness[i], 40, 68) * smoothstep(ratio[i], 0.22, 0.34)
            let crevice = smoothstep(darkness[i], 0.55, 0.70) * smoothstep(blueness[i], 18, 32)
            a = max(blueEdge, crevice)
            if a < 0.10 { a = 0 }
        }
        alpha[i] = a
        guard a > 0 else { continue }
        if x < minX { minX = x }; if x > maxX { maxX = x }
        if y < minY { minY = y }; if y > maxY { maxY = y }
        let p = i * 4
        let obsMax = max(Double(buf[p]), Double(buf[p + 1]), Double(buf[p + 2]))
        if a < 1 && a >= 0.02 && obsMax > 140 {
            for c in 0..<3 {
                let fg = (Double(buf[p + c]) - (1 - a) * 255) / a
                buf[p + c] = UInt8(max(0, min(255, fg)))
            }
        }
    }
}
guard maxX >= minX else { fatalError("no mark pixels found") }
slog("centre holes \(centerHoles)")

// Crop to the mark plus padding.
let x0 = max(0, minX - pad), y0 = max(0, minY - pad)
let x1 = min(w - 1, maxX + pad), y1 = min(h - 1, maxY + pad)
let cw = x1 - x0 + 1, ch = y1 - y0 + 1
var out = [UInt8](repeating: 0, count: cw * ch * 4)
var sumR = 0.0, sumG = 0.0, sumB = 0.0, m = 0.0
for y in 0..<ch {
    for x in 0..<cw {
        let si = ((y0 + y) * w + (x0 + x)) * 4
        let di = (y * cw + x) * 4
        let a = alpha[(y0 + y) * w + (x0 + x)]
        out[di] = buf[si]; out[di + 1] = buf[si + 1]; out[di + 2] = buf[si + 2]
        out[di + 3] = UInt8((a * 255).rounded())
        if a > 0.9 {
            sumR += Double(buf[si]); sumG += Double(buf[si + 1]); sumB += Double(buf[si + 2]); m += 1
        }
    }
}
if m > 0 {
    slog(String(format: "mean core colour #%02X%02X%02X over %d px",
                Int(sumR / m), Int(sumG / m), Int(sumB / m), Int(m)))
}
slog("output \(cw)x\(ch)")

guard let provider = CGDataProvider(data: Data(out) as CFData),
      let outImage = CGImage(width: cw, height: ch, bitsPerComponent: 8, bitsPerPixel: 32,
                             bytesPerRow: cw * 4, space: space,
                             bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                             provider: provider, decode: nil, shouldInterpolate: true,
                             intent: .defaultIntent),
      let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: dstPath) as CFURL,
                                                 "public.png" as CFString, 1, nil)
else { fatalError("cannot write \(dstPath)") }
CGImageDestinationAddImage(dest, outImage, nil)
guard CGImageDestinationFinalize(dest) else { fatalError("finalize") }
slog("wrote \(dstPath)")
