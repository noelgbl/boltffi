import Foundation


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
        mffi_counter_set(handle, value)
    }

    public func increment() {
        mffi_counter_increment(handle)
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
        mffi_datastore_add(handle, point)
    }

    public func len() -> UInt {
        return mffi_datastore_len(handle)
    }

    public func copyInto() -> UInt {
        return mffi_datastore_copy_into(handle)
    }

    public func foreach(callback: (DataPoint) -> Void) {
        mffi_datastore_foreach(handle, callback)
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
        mffi_accumulator_add(handle, amount)
    }

    public func get() -> Int64 {
        return mffi_accumulator_get(handle)
    }

    public func reset() {
        mffi_accumulator_reset(handle)
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
        mffi_sensormonitor_emit_reading(handle, sensorId, timestampMs, value)
    }

    public func subscriberCount() -> UInt {
        return mffi_sensormonitor_subscriber_count(handle)
    }

    public func readings() -> AsyncStream<SensorReading> {
        AsyncStream<SensorReading> { continuation in
    let subscription = mffi_sensormonitor_readings(self.handle)
    let buffer = UnsafeMutablePointer<SensorReading>.allocate(capacity: 16)
    
    continuation.onTermination = { @Sendable _ in
        mffi_sensormonitor_readings_unsubscribe(subscription)
        mffi_sensormonitor_readings_free(subscription)
        buffer.deallocate()
    }
    
    class StreamContext {
        let subscription: SubscriptionHandle
        let buffer: UnsafeMutablePointer<SensorReading>
        let continuation: AsyncStream<SensorReading>.Continuation
        var isActive = true
        
        init(subscription: SubscriptionHandle, buffer: UnsafeMutablePointer<SensorReading>, continuation: AsyncStream<SensorReading>.Continuation) {
            self.subscription = subscription
            self.buffer = buffer
            self.continuation = continuation
        }
    }
    
    let context = StreamContext(subscription: subscription, buffer: buffer, continuation: continuation)
    
    func poll(ctx: StreamContext) {
        guard ctx.isActive else { return }
        
        mffi_sensormonitor_readings_poll(ctx.subscription, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
            let ctx = Unmanaged<StreamContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
            guard ctx.isActive else { return }
            
            let count = mffi_sensormonitor_readings_pop_batch(ctx.subscription, ctx.buffer, 16)
            if count > 0 {
                for i in 0..<Int(count) {
                    ctx.continuation.yield(ctx.buffer[i])
                }
            }
            
            if pollResult == 0 {
                poll(ctx: ctx)
            } else {
                ctx.isActive = false
                ctx.continuation.finish()
            }
        }
    }
    
    poll(ctx: context)
}
    }
}

