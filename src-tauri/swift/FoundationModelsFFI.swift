// C bridge to Apple's on-device model (FoundationModels, macOS 26+).
//
// The framework is Swift-only, so Rust cannot call it directly. This shim
// exposes the two entry points Dictea needs behind @_cdecl.
//
// Everything is guarded by `if #available(macOS 26.0, *)`: the app declares a
// minimum of macOS 10.15 and the framework is weak-linked, so on older systems
// these calls report "unavailable" instead of crashing at launch.
//
// The API is async; Dictea only needs a one-shot blocking call (the text is
// pasted once, there is nothing to stream), so a semaphore is enough. Callers
// must run this off the async runtime — it parks the calling thread.

import Foundation

#if canImport(FoundationModels)
import FoundationModels
#endif

// Availability codes shared with the Rust side
private let FM_AVAILABLE: Int32 = 0
private let FM_UNSUPPORTED_OS: Int32 = 1
private let FM_APPLE_INTELLIGENCE_OFF: Int32 = 2
private let FM_MODEL_NOT_READY: Int32 = 3
private let FM_DEVICE_UNSUPPORTED: Int32 = 4
private let FM_UNKNOWN: Int32 = 5

/// Report whether the on-device model can serve a request right now.
@_cdecl("dictea_fm_availability")
public func dictea_fm_availability() -> Int32 {
    #if canImport(FoundationModels)
    guard #available(macOS 26.0, *) else { return FM_UNSUPPORTED_OS }

    switch SystemLanguageModel.default.availability {
    case .available:
        return FM_AVAILABLE
    case .unavailable(let reason):
        switch reason {
        case .appleIntelligenceNotEnabled:
            return FM_APPLE_INTELLIGENCE_OFF
        case .modelNotReady:
            return FM_MODEL_NOT_READY
        case .deviceNotEligible:
            return FM_DEVICE_UNSUPPORTED
        @unknown default:
            return FM_UNKNOWN
        }
    @unknown default:
        return FM_UNKNOWN
    }
    #else
    return FM_UNSUPPORTED_OS
    #endif
}

/// Run one prompt against the on-device model and block until it answers.
///
/// Returns a malloc'd UTF-8 string that the caller must release with
/// `dictea_fm_free`, or NULL on failure (the reason lands in `error_out`,
/// also owned by the caller).
@_cdecl("dictea_fm_respond")
public func dictea_fm_respond(
    _ instructions: UnsafePointer<CChar>,
    _ prompt: UnsafePointer<CChar>,
    _ error_out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> UnsafeMutablePointer<CChar>? {
    error_out.pointee = nil

    #if canImport(FoundationModels)
    guard #available(macOS 26.0, *) else {
        error_out.pointee = strdup("Apple Intelligence requires macOS 26")
        return nil
    }

    let instructionsText = String(cString: instructions)
    let promptText = String(cString: prompt)

    let semaphore = DispatchSemaphore(value: 0)
    var answer: String?
    var failure: String?

    Task {
        defer { semaphore.signal() }
        do {
            let session = LanguageModelSession(instructions: instructionsText)
            answer = try await session.respond(to: promptText).content
        } catch {
            failure = "\(error)"
        }
    }
    semaphore.wait()

    if let answer {
        return strdup(answer)
    }
    error_out.pointee = strdup(failure ?? "Unknown Apple Intelligence error")
    return nil
    #else
    error_out.pointee = strdup("Built without FoundationModels support")
    return nil
    #endif
}

/// Release a string handed out by this shim
@_cdecl("dictea_fm_free")
public func dictea_fm_free(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr else { return }
    free(ptr)
}
