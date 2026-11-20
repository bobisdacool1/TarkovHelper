import Vision
import AppKit
let path = CommandLine.arguments[1]
guard let img = NSImage(contentsOfFile: path), let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil) else { fatalError("no image") }
let req = VNRecognizeTextRequest()
req.recognitionLevel = .accurate
req.usesLanguageCorrection = false
req.minimumTextHeight = 0.01
try VNImageRequestHandler(cgImage: cg).perform([req])
let W = Double(cg.width), H = Double(cg.height)
for r in (req.results ?? []) {
  guard let c = r.topCandidates(1).first else { continue }
  let b = r.boundingBox
  print(String(format: "%@\t%.2f\tx=%d y=%d w=%d h=%d", c.string, c.confidence, Int(b.minX*W), Int((1-b.maxY)*H), Int(b.width*W), Int(b.height*H)))
}
