import Foundation

public struct FfiError: Error {
    public let status: FfiStatus
    public let message: String
    public init(status: FfiStatus, message: String = "") { self.status = status; self.message = message }
}

@inline(__always)
private func stringFromFfi(_ ffiString: FfiString) -> String {
    guard ffiString.len > 0, let pointer = ffiString.ptr else { return "" }
    return String(decoding: UnsafeBufferPointer(start: pointer, count: Int(ffiString.len)), as: UTF8.self)
}

@inline(__always)
private func lastErrorMessage() -> String? {
    var errorString = FfiString(ptr: nil, len: 0, cap: 0)
    let status = mffi_last_error_message(&errorString)
    defer { mffi_free_string(errorString); mffi_clear_last_error() }
    guard status.code == 0 else { return nil }
    return stringFromFfi(errorString)
}

@inline(__always)
private func checkStatus(_ status: FfiStatus, context: StaticString = #function) throws {
    guard status.code == 0 else {
        let message = lastErrorMessage() ?? ""
        throw FfiError(status: status, message: message.isEmpty ? "FFI failed in \(context)" : message)
    }
}

@inline(__always)
private func ensureOk(_ status: FfiStatus, context: StaticString = #function) {
    guard status.code == 0 else {
        let message = lastErrorMessage() ?? ""
        fatalError(message.isEmpty ? "FFI failed in \(context) [\(status.code)]" : message)
    }
}
final class FfiFutureState<T>: @unchecked Sendable {
    typealias Continuation = CheckedContinuation<T, Error>

    enum FinishDecision {
        case alreadyFinished
        case finishWithoutContinuation
        case finishWithContinuation(Continuation)
    }

    final class ContinuationBox {
        let continuation: Continuation
        init(_ continuation: Continuation) { self.continuation = continuation }
    }

    let handle: RustFutureHandle?
    private var continuationSlot: UInt64 = 0

    init(handle: RustFutureHandle?) {
        self.handle = handle
    }

    func installContinuation(_ continuation: Continuation) -> Bool {
        let box = ContinuationBox(continuation)
        let raw = UInt64(UInt(bitPattern: Unmanaged.passRetained(box).toOpaque()))
        let prior = withUnsafeMutablePointer(to: &continuationSlot) { mffi_atomic_u64_exchange($0, raw) }

        if prior == 0 { return true }
        if prior == 1 {
            Unmanaged.passUnretained(box).release()
            return false
        }
        withUnsafeMutablePointer(to: &continuationSlot) { _ = mffi_atomic_u64_exchange($0, prior) }
        Unmanaged.passUnretained(box).release()
        return false
    }

    @inline(__always)
    func canPoll() -> Bool {
        withUnsafeMutablePointer(to: &continuationSlot) { mffi_atomic_u64_load($0) } != 1
    }

    func decideFinish() -> FinishDecision {
        let prior = withUnsafeMutablePointer(to: &continuationSlot) { mffi_atomic_u64_exchange($0, 1) }
        if prior == 1 { return .alreadyFinished }
        if prior == 0 { return .finishWithoutContinuation }
        let box = Unmanaged<ContinuationBox>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(prior))!).takeRetainedValue()
        return .finishWithContinuation(box.continuation)
    }
}

public func greeting(name: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
	    return name.withCString { namePtr in
	    let status = mffi_greeting(UnsafeRawPointer(namePtr).assumingMemoryBound(to: UInt8.self), UInt(name.utf8.count), &result)
	    defer { mffi_free_string(result) }
	    ensureOk(status)
	    return stringFromFfi(result)
    }
}

public func concat(first: String, second: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
	    return first.withCString { firstPtr in
	    second.withCString { secondPtr in
	    let status = mffi_concat(UnsafeRawPointer(firstPtr).assumingMemoryBound(to: UInt8.self), UInt(first.utf8.count), UnsafeRawPointer(secondPtr).assumingMemoryBound(to: UInt8.self), UInt(second.utf8.count), &result)
	    defer { mffi_free_string(result) }
	    ensureOk(status)
	    return stringFromFfi(result)
    }
    }
}

public func reverseString(input: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
	    return input.withCString { inputPtr in
	    let status = mffi_reverse_string(UnsafeRawPointer(inputPtr).assumingMemoryBound(to: UInt8.self), UInt(input.utf8.count), &result)
	    defer { mffi_free_string(result) }
	    ensureOk(status)
	    return stringFromFfi(result)
    }
}

public func copyBytes(src: [UInt8], dst: inout [UInt8]) -> UInt {
	    return src.withUnsafeBufferPointer { srcPtr in
	    dst.withUnsafeMutableBufferPointer { dstPtr in
	    return mffi_copy_bytes(srcPtr.baseAddress, UInt(srcPtr.count), dstPtr.baseAddress, UInt(dstPtr.count))
    }
    }
}

public func addNumbers(first: Int32, second: Int32) -> Int32 {
	    return mffi_add_numbers(first, second)
}

public func multiplyFloats(first: Double, second: Double) -> Double {
	    return mffi_multiply_floats(first, second)
}

public func makeGreeting(name: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
	    return name.withCString { namePtr in
	    let status = mffi_make_greeting(UnsafeRawPointer(namePtr).assumingMemoryBound(to: UInt8.self), UInt(name.utf8.count), &result)
	    defer { mffi_free_string(result) }
	    ensureOk(status)
	    return stringFromFfi(result)
    }
}

public func safeDivide(numerator: Int32, denominator: Int32) throws -> Int32 {
	    var outValue: Int32 = 0
	    let status = mffi_safe_divide(numerator, denominator, &outValue)
	    try checkStatus(status)
	    return outValue
}

public func generateSequence(count: Int32) -> [Int32] {
	    let len = mffi_generate_sequence_len(count)
	    var arr = [Int32](repeating: 0, count: Int(len))
	    var written: UInt = 0
	    let status = mffi_generate_sequence_copy_into(count, &arr, len, &written)
	    ensureOk(status)
	    let writtenCount = min(Int(written), arr.count)
	    if writtenCount < arr.count { arr.removeSubrange(writtenCount..<arr.count) }
	    return arr
}

public func foreachRange(start: Int32, end: Int32, callback: @escaping (Int32) -> Void) {
    typealias ForeachRangeCallbackFn = (Int32) -> Void
    class ForeachRangeCallbackBox { let fn_: ForeachRangeCallbackFn; init(_ fn_: @escaping ForeachRangeCallbackFn) { self.fn_ = fn_ } }
    let callbackBox = ForeachRangeCallbackBox(callback)
    let callbackPtr = Unmanaged.passRetained(callbackBox).toOpaque()
    let callbackTrampoline: @convention(c) (UnsafeMutableRawPointer?, Int32) -> Void = { ud, val in
        Unmanaged<ForeachRangeCallbackBox>.fromOpaque(ud!).takeUnretainedValue().fn_(val)
    }
	    let status = mffi_foreach_range(start, end, callbackTrampoline, callbackPtr)
	    Unmanaged<ForeachRangeCallbackBox>.fromOpaque(callbackPtr).release()
	    ensureOk(status)
}

public func oppositeDirection(dir: Direction) -> Direction {
	    return mffi_opposite_direction(dir)
}

public func directionToDegrees(dir: Direction) -> Int32 {
	    return mffi_direction_to_degrees(dir)
}

public func findEven(value: Int32) -> Int32? {
	    var outValue: Int32 = 0
	    let isSome = mffi_find_even(value, &outValue)
	    return isSome != 0 ? outValue : nil
}

public func processValue(value: Int32) -> ApiResult {
	    return mffi_process_value(value)
}

public func apiResultIsSuccess(result: ApiResult) -> Bool {
	    return mffi_api_result_is_success(result)
}

public func computeHeavy(input: Int32) async throws -> Int32 {
    let futureHandle =
            mffi_compute_heavy(input)
    
    typealias FutureContext = FfiFutureState<Int32>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_compute_heavy_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let result = mffi_compute_heavy_complete(ctx.handle, &status)
                        mffi_compute_heavy_free(ctx.handle)
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                continuation.resume(returning: result)
                            }
                        case .finishWithoutContinuation:
                            break
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_compute_heavy_cancel(state.handle)
            mffi_compute_heavy_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_compute_heavy_cancel(state.handle)
            mffi_compute_heavy_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}

public func fetchData(id: Int32) async throws -> Int32 {
    let futureHandle =
            mffi_fetch_data(id)
    
    typealias FutureContext = FfiFutureState<Int32>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_fetch_data_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let result = mffi_fetch_data_complete(ctx.handle, &status)
                        mffi_fetch_data_free(ctx.handle)
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                continuation.resume(returning: result)
                            }
                        case .finishWithoutContinuation:
                            break
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_fetch_data_cancel(state.handle)
            mffi_fetch_data_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_fetch_data_cancel(state.handle)
            mffi_fetch_data_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}

public func asyncMakeString(value: Int32) async throws -> String {
    let futureHandle =
            mffi_async_make_string(value)
    
    typealias FutureContext = FfiFutureState<String>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_async_make_string_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let ffiStr = mffi_async_make_string_complete(ctx.handle, &status)
                        mffi_async_make_string_free(ctx.handle)
                        defer { mffi_free_string(ffiStr) }
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                let str = stringFromFfi(ffiStr)
                                continuation.resume(returning: str)
                            }
                        case .finishWithoutContinuation:
                            break
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_async_make_string_cancel(state.handle)
            mffi_async_make_string_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_async_make_string_cancel(state.handle)
            mffi_async_make_string_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}

public func asyncFetchPoint(x: Double, y: Double) async throws -> DataPoint {
    let futureHandle =
            mffi_async_fetch_point(x, y)
    
    typealias FutureContext = FfiFutureState<DataPoint>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_async_fetch_point_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let result = mffi_async_fetch_point_complete(ctx.handle, &status)
                        mffi_async_fetch_point_free(ctx.handle)
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                continuation.resume(returning: result)
                            }
                        case .finishWithoutContinuation:
                            break
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_async_fetch_point_cancel(state.handle)
            mffi_async_fetch_point_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_async_fetch_point_cancel(state.handle)
            mffi_async_fetch_point_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}

public func asyncGetNumbers(count: Int32) async throws -> [Int32] {
    let futureHandle =
            mffi_async_get_numbers(count)
    
    typealias FutureContext = FfiFutureState<[Int32]>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_async_get_numbers_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let buf = mffi_async_get_numbers_complete(ctx.handle, &status)
                        mffi_async_get_numbers_free(ctx.handle)
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                mffi_free_buf_i32(buf)
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                let arr = Array(UnsafeBufferPointer(start: buf.ptr, count: Int(buf.len)))
                                mffi_free_buf_i32(buf)
                                continuation.resume(returning: arr)
                            }
                        case .finishWithoutContinuation:
                            mffi_free_buf_i32(buf)
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_async_get_numbers_cancel(state.handle)
            mffi_async_get_numbers_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_async_get_numbers_cancel(state.handle)
            mffi_async_get_numbers_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}

public func asyncFindValue(needle: Int32) async throws -> Int32? {
    let futureHandle =
            mffi_async_find_value(needle)
    
    typealias FutureContext = FfiFutureState<Int32?>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_async_find_value_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let opt = mffi_async_find_value_complete(ctx.handle, &status)
                        mffi_async_find_value_free(ctx.handle)
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                continuation.resume(returning: opt.isSome ? opt.value : nil)
                            }
                        case .finishWithoutContinuation:
                            break
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_async_find_value_cancel(state.handle)
            mffi_async_find_value_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_async_find_value_cancel(state.handle)
            mffi_async_find_value_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}

public func asyncGreeting(name: String) async throws -> String {
    let futureHandle =
        name.withCString { namePtr in
            mffi_async_greeting(UnsafeRawPointer(namePtr).assumingMemoryBound(to: UInt8.self), UInt(name.utf8.count))
        }
    
    typealias FutureContext = FfiFutureState<String>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_async_greeting_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let ffiStr = mffi_async_greeting_complete(ctx.handle, &status)
                        mffi_async_greeting_free(ctx.handle)
                        defer { mffi_free_string(ffiStr) }
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                let str = stringFromFfi(ffiStr)
                                continuation.resume(returning: str)
                            }
                        case .finishWithoutContinuation:
                            break
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_async_greeting_cancel(state.handle)
            mffi_async_greeting_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_async_greeting_cancel(state.handle)
            mffi_async_greeting_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}

public func asyncFetchNumbers(id: Int32) async throws -> [Int32] {
    let futureHandle =
            mffi_async_fetch_numbers(id)
    
    typealias FutureContext = FfiFutureState<[Int32]>
    let state = FutureContext(handle: futureHandle)
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.installContinuation(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            
            func poll(ctx: FutureContext) {
                guard ctx.canPoll() else { return }
                mffi_async_fetch_numbers_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        let decision = ctx.decideFinish()
                        if case .alreadyFinished = decision { return }
                        var status = FfiStatus()
                        let buf = mffi_async_fetch_numbers_complete(ctx.handle, &status)
                        mffi_async_fetch_numbers_free(ctx.handle)
                        switch decision {
                        case .finishWithContinuation(let continuation):
                            if status.code != 0 {
                                mffi_free_buf_i32(buf)
                                let message = lastErrorMessage() ?? ""
                                continuation.resume(throwing: FfiError(status: status, message: message))
                            } else {
                                let arr = Array(UnsafeBufferPointer(start: buf.ptr, count: Int(buf.len)))
                                mffi_free_buf_i32(buf)
                                continuation.resume(returning: arr)
                            }
                        case .finishWithoutContinuation:
                            mffi_free_buf_i32(buf)
                        case .alreadyFinished:
                            break
                        }
                    } else {
                        guard ctx.canPoll() else { return }
                        Task { [ctx] in
                            await Task.yield()
                            poll(ctx: ctx)
                        }
                    }
                }
            }
            poll(ctx: state)
        }
    } onCancel: {
        let decision = state.decideFinish()
        switch decision {
        case .alreadyFinished:
            break
        case .finishWithoutContinuation:
            mffi_async_fetch_numbers_cancel(state.handle)
            mffi_async_fetch_numbers_free(state.handle)
        case .finishWithContinuation(let continuation):
            mffi_async_fetch_numbers_cancel(state.handle)
            mffi_async_fetch_numbers_free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}


public final class Counter {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_counter_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_counter_free(handle)
    }

    public func set(value: UInt64) {
        
let status = mffi_counter_set(handle, value)
ensureOk(status)
    }

    public func increment() {
        
let status = mffi_counter_increment(handle)
ensureOk(status)
    }

    public func get() -> UInt64 {
        return mffi_counter_get(handle)
    }
}


public final class DataStore {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_datastore_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_datastore_free(handle)
    }

    public func add(point: DataPoint) {
        
let status = mffi_datastore_add(handle, point)
ensureOk(status)
    }

    public func len() -> UInt {
        return mffi_datastore_len(handle)
    }

    public func copyInto(dst: inout [DataPoint]) -> UInt {
        
return dst.withUnsafeMutableBufferPointer { dstPtr in
mffi_datastore_copy_into(handle, dstPtr.baseAddress, UInt(dstPtr.count))
}
    }

    public func foreach(callback: @escaping (DataPoint) -> Void) {
        
        typealias ForeachCallbackFn = (DataPoint) -> Void
        class ForeachCallbackBox { let fn_: ForeachCallbackFn; init(_ fn_: @escaping ForeachCallbackFn) { self.fn_ = fn_ } }
        let callbackBox = ForeachCallbackBox(callback)
        let callbackPtr = Unmanaged.passRetained(callbackBox).toOpaque()
        defer { Unmanaged<ForeachCallbackBox>.fromOpaque(callbackPtr).release() }
        let callbackTrampoline: @convention(c) (UnsafeMutableRawPointer?, DataPoint) -> Void = { ud, val in
            Unmanaged<ForeachCallbackBox>.fromOpaque(ud!).takeUnretainedValue().fn_(val)
        }
        let status = mffi_datastore_foreach(handle, callbackTrampoline, callbackPtr)
        ensureOk(status)
    }

    public func sum() -> Double {
        return mffi_datastore_sum(handle)
    }
}


public final class Accumulator {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_accumulator_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_accumulator_free(handle)
    }

    public func add(amount: Int64) {
        
let status = mffi_accumulator_add(handle, amount)
ensureOk(status)
    }

    public func get() -> Int64 {
        return mffi_accumulator_get(handle)
    }

    public func reset() {
        
let status = mffi_accumulator_reset(handle)
ensureOk(status)
    }
}


public final class SensorMonitor {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_sensormonitor_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_sensormonitor_free(handle)
    }

    public func emitReading(sensorId: Int32, timestampMs: Int64, value: Double) {
        
let status = mffi_sensormonitor_emit_reading(handle, sensorId, timestampMs, value)
ensureOk(status)
    }

    public func subscriberCount() -> UInt {
        return mffi_sensormonitor_subscriber_count(handle)
    }

    public func readings() -> AsyncStream<SensorReading> {
        AsyncStream<SensorReading> { continuation in
    guard let subscription = mffi_sensormonitor_readings(self.handle) else {
        continuation.finish()
        return
    }

    final class StreamContext: @unchecked Sendable {
        let subscription: SubscriptionHandle
        let bufferCapacity: UInt
        let buffer: UnsafeMutablePointer<SensorReading>
        private var lifecycleTag: UInt8 = 0
        private var callbackTag: UInt8 = 0
        private var continuation: AsyncStream<SensorReading>.Continuation?

        init(
            subscription: SubscriptionHandle,
            bufferCapacity: UInt,
            continuation: AsyncStream<SensorReading>.Continuation
        ) {
            self.subscription = subscription
            self.bufferCapacity = bufferCapacity
            self.buffer = UnsafeMutablePointer<SensorReading>.allocate(capacity: Int(bufferCapacity))
            self.continuation = continuation
        }

        func start() {
            registerPoll()
        }

        func requestTermination() {
            let terminationStarted = withUnsafeMutablePointer(to: &lifecycleTag) { mffi_atomic_u8_cas($0, 0, 1) }
            if terminationStarted {
                mffi_sensormonitor_readings_unsubscribe(subscription)
                _ = withUnsafeMutablePointer(to: &lifecycleTag) { mffi_atomic_u8_cas($0, 1, 2) }
            }
            attemptFinalize()
        }

        private func attemptFinalize() {
            guard (withUnsafeMutablePointer(to: &callbackTag) { mffi_atomic_u8_cas($0, 0, 0) }) else { return }
            guard (withUnsafeMutablePointer(to: &lifecycleTag) { mffi_atomic_u8_cas($0, 2, 3) }) else { return }
            mffi_sensormonitor_readings_free(subscription)
            buffer.deallocate()
            continuation?.finish()
            continuation = nil
        }

        private func schedulePoll() {
            Task { [self] in
                await Task.yield()
                registerPoll()
            }
        }

        private func registerPoll() {
            guard (withUnsafeMutablePointer(to: &lifecycleTag) { mffi_atomic_u8_cas($0, 0, 0) }) else {
                attemptFinalize()
                return
            }

            let callbackData = UInt64(UInt(bitPattern: Unmanaged.passRetained(self).toOpaque()))
            mffi_sensormonitor_readings_poll(subscription, callbackData) { callbackData, pollResult in
                let context = Unmanaged<StreamContext>
                    .fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!)
                    .takeRetainedValue()
                context.handlePoll(pollResult: pollResult)
            }
        }

        private func handlePoll(pollResult: Int8) {
            let isClosed = pollResult == 1

            let entered = withUnsafeMutablePointer(to: &callbackTag) { mffi_atomic_u8_cas($0, 0, 1) }
            guard entered else {
                attemptFinalize()
                return
            }

            defer {
                _ = withUnsafeMutablePointer(to: &callbackTag) { mffi_atomic_u8_cas($0, 1, 0) }
                attemptFinalize()
            }

            guard (withUnsafeMutablePointer(to: &lifecycleTag) { mffi_atomic_u8_cas($0, 0, 0) }) else { return }
            guard let continuation = continuation else { return }

            while true {
                let count = mffi_sensormonitor_readings_pop_batch(subscription, buffer, bufferCapacity)
                guard count > 0 else { break }
                UnsafeBufferPointer(start: buffer, count: Int(count)).forEach { element in
                    _ = continuation.yield(element)
                }
            }

            if isClosed {
                requestTermination()
                return
            }

            guard (withUnsafeMutablePointer(to: &lifecycleTag) { mffi_atomic_u8_cas($0, 0, 0) }) else { return }
            schedulePoll()
        }
    }

    let context = StreamContext(subscription: subscription, bufferCapacity: 16, continuation: continuation)
    continuation.onTermination = { @Sendable _ in
        context.requestTermination()
    }
    context.start()
}
    }
}


public final class DataConsumer {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_dataconsumer_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_dataconsumer_free(handle)
    }

    public func setProvider(provider: DataProviderProtocol) {
        
let status = mffi_dataconsumer_set_provider(handle, UnsafeMutablePointer<ForeignDataProvider>(DataProviderBridge.create(provider)))
ensureOk(status)
    }

    public func computeSum() -> UInt64 {
        return mffi_dataconsumer_compute_sum(handle)
    }
}


public protocol DataProviderProtocol: AnyObject {
    func getCount() -> UInt32
    func getItem(index: UInt32) -> DataPoint
}

private class DataProviderWrapper {
    let impl_: DataProviderProtocol
    init(_ impl_: DataProviderProtocol) { self.impl_ = impl_ }
}

private var dataProviderVTableInstance: DataProviderVTable = {
    DataProviderVTable(
        free: { handle in
            guard handle != 0 else { return }
            Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).release()
        },
        clone: { handle in
            guard handle != 0 else { return 0 }
            let wrapper = Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!)
            _ = wrapper.retain()
            return handle
        },
        get_count: { handle, outPtr, statusPtr in
            guard handle != 0 else { statusPtr?.pointee = FfiStatus(code: 1); return }
            let wrapper = Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            let result = wrapper.impl_.getCount()
            outPtr?.pointee = result
            statusPtr?.pointee = FfiStatus(code: 0)
        },
        get_item: { handle, index, outPtr, statusPtr in
            guard handle != 0 else { statusPtr?.pointee = FfiStatus(code: 1); return }
            let wrapper = Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            let result = wrapper.impl_.getItem(index: index)
            outPtr?.pointee = result
            statusPtr?.pointee = FfiStatus(code: 0)
        }
    )
}()

public enum DataProviderBridge {
    private static var isRegistered = false
    
    public static func register() {
        guard !isRegistered else { return }
        mffi_register_data_provider_vtable(&dataProviderVTableInstance)
        isRegistered = true
    }
    
    public static func create(_ impl: DataProviderProtocol) -> OpaquePointer {
        register()
        let wrapper = DataProviderWrapper(impl)
        let handle = UInt64(UInt(bitPattern: Unmanaged.passRetained(wrapper).toOpaque()))
        return OpaquePointer(mffi_create_data_provider(handle)!)
    }
}


public protocol AsyncDataFetcherProtocol: AnyObject {
    func fetchValue(key: UInt32) async -> UInt64
}

private class AsyncDataFetcherWrapper {
    let impl_: AsyncDataFetcherProtocol
    init(_ impl_: AsyncDataFetcherProtocol) { self.impl_ = impl_ }
}

private var asyncDataFetcherVTableInstance: AsyncDataFetcherVTable = {
    AsyncDataFetcherVTable(
        free: { handle in
            guard handle != 0 else { return }
            Unmanaged<AsyncDataFetcherWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).release()
        },
        clone: { handle in
            guard handle != 0 else { return 0 }
            let wrapper = Unmanaged<AsyncDataFetcherWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!)
            _ = wrapper.retain()
            return handle
        },
        fetch_value: { handle, key, callback, callbackData in
            guard handle != 0 else { return }
            let wrapper = Unmanaged<AsyncDataFetcherWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            Task {
                let result = await wrapper.impl_.fetchValue(key: key)
                callback?(callbackData, result, FfiStatus(code: 0))
            }
        }
    )
}()

public enum AsyncDataFetcherBridge {
    private static var isRegistered = false
    
    public static func register() {
        guard !isRegistered else { return }
        mffi_register_async_data_fetcher_vtable(&asyncDataFetcherVTableInstance)
        isRegistered = true
    }
    
    public static func create(_ impl: AsyncDataFetcherProtocol) -> OpaquePointer {
        register()
        let wrapper = AsyncDataFetcherWrapper(impl)
        let handle = UInt64(UInt(bitPattern: Unmanaged.passRetained(wrapper).toOpaque()))
        return OpaquePointer(mffi_create_async_data_fetcher(handle)!)
    }
}

