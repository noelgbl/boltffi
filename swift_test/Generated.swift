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

    public func foreach(callback: @escaping (DataPoint) -> Void) {
        
        typealias ForeachCallbackFn = (DataPoint) -> Void
        class ForeachCallbackBox { let fn_: ForeachCallbackFn; init(_ fn_: @escaping ForeachCallbackFn) { self.fn_ = fn_ } }
        let callbackBox = ForeachCallbackBox(callback)
        let callbackPtr = Unmanaged.passRetained(callbackBox).toOpaque()
        let callbackTrampoline: @convention(c) (UnsafeMutableRawPointer?, DataPoint) -> Void = { ud, val in
            Unmanaged<ForeachCallbackBox>.fromOpaque(ud!).takeUnretainedValue().fn_(val)
        }
        let _ = mffi_datastore_foreach(handle, callbackTrampoline, callbackPtr)
        Unmanaged<ForeachCallbackBox>.fromOpaque(callbackPtr).release()
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
        return mffi_create_data_provider(handle)!
    }
}


public protocol NavigationObserverProtocol: AnyObject {
    func onLocationUpdated(lat: Double, lon: Double)
    func onRouteChanged(routeId: UInt32)
    func onError(code: Int32, message: String)
}

private class NavigationObserverWrapper {
    let impl_: NavigationObserverProtocol
    init(_ impl_: NavigationObserverProtocol) { self.impl_ = impl_ }
}

private var navigationObserverVTableInstance: NavigationObserverVTable = {
    NavigationObserverVTable(
        free: { handle in
            guard handle != 0 else { return }
            Unmanaged<NavigationObserverWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).release()
        },
        clone: { handle in
            guard handle != 0 else { return 0 }
            let wrapper = Unmanaged<NavigationObserverWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!)
            _ = wrapper.retain()
            return handle
        },
        on_location_updated: { handle, lat, lon, statusPtr in
            guard handle != 0 else { statusPtr?.pointee = FfiStatus(code: 1); return }
            let wrapper = Unmanaged<NavigationObserverWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            wrapper.impl_.onLocationUpdated(lat: lat, lon: lon)
            statusPtr?.pointee = FfiStatus(code: 0)
        },
        on_route_changed: { handle, route_id, statusPtr in
            guard handle != 0 else { statusPtr?.pointee = FfiStatus(code: 1); return }
            let wrapper = Unmanaged<NavigationObserverWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            wrapper.impl_.onRouteChanged(routeId: routeId)
            statusPtr?.pointee = FfiStatus(code: 0)
        },
        on_error: { handle, code, message, statusPtr in
            guard handle != 0 else { statusPtr?.pointee = FfiStatus(code: 1); return }
            let wrapper = Unmanaged<NavigationObserverWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            wrapper.impl_.onError(code: code, message: message)
            statusPtr?.pointee = FfiStatus(code: 0)
        }
    )
}()

public enum NavigationObserverBridge {
    private static var isRegistered = false
    
    public static func register() {
        guard !isRegistered else { return }
        mffi_register_navigation_observer_vtable(&navigationObserverVTableInstance)
        isRegistered = true
    }
    
    public static func create(_ impl: NavigationObserverProtocol) -> OpaquePointer {
        register()
        let wrapper = NavigationObserverWrapper(impl)
        let handle = UInt64(UInt(bitPattern: Unmanaged.passRetained(wrapper).toOpaque()))
        return mffi_create_navigation_observer(handle)!
    }
}

