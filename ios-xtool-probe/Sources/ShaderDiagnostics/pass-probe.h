// Own-process synthetic metadata probe. No PKPassLibrary or Wallet service use.
@protocol NativePassInitializing <NSObject>
- (instancetype)initWithDictionary:(NSDictionary *)dictionary bundle:(NSBundle *)bundle;
@end

@interface ShaderRecordingDictionary : NSDictionary {
    NSDictionary *_storage;
    NSMutableSet *_keys;
    BOOL _rejected, _truncated;
}
- (instancetype)initWithStorage:(NSDictionary *)storage;
- (NSDictionary *)trace;
@end
@implementation ShaderRecordingDictionary
- (instancetype)initWithObjects:(const id __unsafe_unretained [])objects forKeys:(const id<NSCopying> __unsafe_unretained [])keys count:(NSUInteger)count {
    self = [super init];
    if (self) { _storage = [[NSDictionary alloc] initWithObjects:objects forKeys:keys count:count]; _keys = [NSMutableSet set]; }
    return self;
}
- (instancetype)initWithStorage:(NSDictionary *)storage {
    self = [self initWithObjects:NULL forKeys:NULL count:0];
    if (self) { _storage = [storage copy]; _keys = [NSMutableSet set]; }
    return self;
}
- (NSUInteger)count { return _storage.count; }
- (NSEnumerator *)keyEnumerator { return _storage.keyEnumerator; }
- (id)objectForKey:(id)key {
    if (![key isKindOfClass:NSString.class]) _rejected = YES;
    else {
        NSRegularExpression *pattern = [NSRegularExpression regularExpressionWithPattern:@"\\A[A-Za-z_][A-Za-z0-9_]{0,63}\\z" options:0 error:nil];
        NSRange range = NSMakeRange(0, [key length]);
        if (![pattern numberOfMatchesInString:key options:0 range:range]) _rejected = YES;
        else if (_keys.count < 128) [_keys addObject:[key copy]]; else _truncated = YES;
    }
    return [_storage objectForKey:key];
}
- (id)copyWithZone:(NSZone *)zone { return self; }
- (id)mutableCopyWithZone:(NSZone *)zone { return [_storage mutableCopyWithZone:zone]; }
- (NSDictionary *)trace {
    return @{@"keys": [[_keys allObjects] sortedArrayUsingSelector:@selector(compare:)],
        @"rejected": @(_rejected), @"truncated": @(_truncated)};
}
@end

static BOOL ScalarGetter(Class cls, SEL selector, const char *expected) {
    Method method = class_getInstanceMethod(cls, selector);
    if (!method || method_getNumberOfArguments(method) != 2) return NO;
    char result[64]; method_getReturnType(method, result, sizeof(result));
    return strcmp(result, expected) == 0;
}

static NSDictionary *ConstructSyntheticPass(Class passClass, NSDictionary *metadata, NSBundle *bundle, id accessor) {
    NSMutableDictionary *result = [@{@"status": @"unknown", @"pass_created": @NO,
        @"exception": @NO, @"live_background": @NO, @"live_render_type": @0,
        @"shader_object_returned": @NO, @"shader_source_preserved": @NO} mutableCopy];
    @try {
        id pass = [(id<NativePassInitializing>)[passClass alloc] initWithDictionary:metadata bundle:bundle];
        if (!pass || ![pass isKindOfClass:passClass]) return result;
        result[@"pass_created"] = @YES;
        Class concrete = object_getClass(pass);
        if (accessor) {
            SEL setAccessor = NSSelectorFromString(@"setDataAccessor:");
            Method setter = class_getInstanceMethod(concrete, setAccessor);
            char returnType[64] = {0}, argument[64] = {0};
            if (!setter || method_getNumberOfArguments(setter) != 3) return result;
            method_getReturnType(setter, returnType, sizeof(returnType));
            method_getArgumentType(setter, 2, argument, sizeof(argument));
            if (returnType[0] != 'v' || argument[0] != '@') return result;
            void (*bindAccessor)(id, SEL, id) = (void *)method_getImplementation(setter);
            bindAccessor(pass, setAccessor, accessor); // Genuine own-file accessor only.
        }
        SEL background = NSSelectorFromString(@"liveRenderedBackground");
        SEL renderType = NSSelectorFromString(@"liveRenderType");
        SEL shaderSet = NSSelectorFromString(@"liveRenderedShaderSet");
        if (!ScalarGetter(concrete, background, @encode(BOOL)) ||
            !ScalarGetter(concrete, renderType, @encode(NSUInteger)) ||
            !ObjectMethod(concrete, shaderSet, 0)) return result;
        BOOL (*getBackground)(id, SEL) = (void *)class_getMethodImplementation(concrete, background);
        NSUInteger (*getType)(id, SEL) = (void *)class_getMethodImplementation(concrete, renderType);
        id (*getShaders)(id, SEL) = (void *)class_getMethodImplementation(concrete, shaderSet);
        NSUInteger type = getType(pass, renderType);
        if (type > 1024) return result;
        result[@"live_render_type"] = @(type); // Observed native value; never assigned.
        result[@"live_background"] = @(getBackground(pass, background));
        id shaders = getShaders(pass, shaderSet);
        Class nativeShaders = NSClassFromString(@"PKLiveRenderedShaderSet");
        if (shaders && ![shaders isKindOfClass:nativeShaders]) return result;
        result[@"shader_object_returned"] = @((BOOL)(shaders != nil));
        BOOL allEqual = shaders != nil;
        if (shaders) {
            for (NSString *name in @[@"fragmentEntryPointShader", @"surfaceEntryPointShader", @"geometryEntryPointShader", @"lightingModelEntryPointShader"]) {
                SEL selector = NSSelectorFromString(name);
                if (!ObjectMethod(nativeShaders, selector, 0)) { allEqual = NO; break; }
                id (*getter)(id, SEL) = (void *)class_getMethodImplementation(nativeShaders, selector);
                id value = getter(shaders, selector);
                if (![value isKindOfClass:NSString.class] || ![value isEqual:@"// Wallet Studio synthetic loader control\n"]) allEqual = NO;
            }
        }
        result[@"shader_source_preserved"] = @(allEqual);
        result[@"status"] = @"observed";
    } @catch (NSException *exception) { result[@"exception"] = @YES; }
    return result;
}

static NSDictionary *ProbeSyntheticPass(NSDictionary *loader) {
    NSMutableDictionary *result = [@{@"status": @"unknown", @"reason": @"loader_control_unavailable",
        @"reference": @{}, @"observer": @{}, @"parity": @NO,
        @"metadata_lookup_keys": @[], @"trace_rejected": @NO, @"trace_truncated": @NO,
        @"bound_reference": @{}, @"bound_observer": @{}, @"bound_parity": @NO,
        @"live_reference": @{}, @"live_observer": @{}, @"live_parity": @NO} mutableCopy];
    if (![loader[@"source_preserved"] boolValue]) return result;
    Class passClass = NSClassFromString(@"PKPass");
    if (!passClass || !ObjectMethod(passClass, @selector(initWithDictionary:bundle:), 2)) {
        result[@"reason"] = @"pass_signature_unavailable"; return result;
    }
    NSURL *root = [[NSFileManager defaultManager] URLsForDirectory:NSCachesDirectory inDomains:NSUserDomainMask].firstObject;
    NSURL *directory = [root URLByAppendingPathComponent:@"WalletStudioSyntheticShaders.pkpass" isDirectory:YES];
    // Public Apple generic-pass keys only, all values owned by the diagnostic.
    // This is unsigned synthetic metadata, never submitted to Wallet.
    NSDictionary *metadata = @{@"formatVersion": @1, @"passTypeIdentifier": @"pass.local.walletstudio.synthetic",
        @"serialNumber": @"synthetic-only", @"teamIdentifier": @"ABCDEFGHIJ", @"organizationName": @"Wallet Studio",
        @"description": @"Synthetic shader diagnostic", @"generic": @{@"primaryFields": @[]}};
    @try {
        NSData *json = [NSJSONSerialization dataWithJSONObject:metadata options:0 error:nil];
        if (![json writeToURL:[directory URLByAppendingPathComponent:@"pass.json"] atomically:YES]) {
            result[@"reason"] = @"pass_fixture_unavailable"; return result;
        }
        NSBundle *bundle = [NSBundle bundleWithURL:directory];
        if (!bundle) { result[@"reason"] = @"pass_fixture_unavailable"; return result; }
        ShaderRecordingDictionary *observer = [[ShaderRecordingDictionary alloc] initWithStorage:metadata];
        result[@"reference"] = ConstructSyntheticPass(passClass, metadata, bundle, nil);
        result[@"observer"] = ConstructSyntheticPass(passClass, observer, bundle, nil);
        result[@"parity"] = @((BOOL)[result[@"reference"] isEqual:result[@"observer"]]);
        NSDictionary *trace = observer.trace;
        result[@"trace_rejected"] = trace[@"rejected"];
        result[@"trace_truncated"] = trace[@"truncated"];
        BOOL observed = [result[@"reference"][@"status"] isEqual:@"observed"] && [result[@"parity"] boolValue] &&
            ![trace[@"rejected"] boolValue] && ![trace[@"truncated"] boolValue];
        if (observed) {
            result[@"metadata_lookup_keys"] = trace[@"keys"];
            result[@"status"] = @"observed";
            result[@"reason"] = @"synthetic_generic_pass_observed";
            Class accessorClass = NSClassFromString(@"PKFileDataAccessor");
            NSError *accessorError = nil;
            // The same initializer/signature was validated by the v4 loader.
            id accessor = [(id<NativeFileAccessorInitializing>)[accessorClass alloc] initWithFileURL:directory error:&accessorError];
            if (accessor) {
                result[@"bound_reference"] = ConstructSyntheticPass(passClass, metadata, bundle, accessor);
                result[@"bound_observer"] = ConstructSyntheticPass(passClass, [[ShaderRecordingDictionary alloc] initWithStorage:metadata], bundle, accessor);
                result[@"bound_parity"] = @((BOOL)[result[@"bound_reference"] isEqual:result[@"bound_observer"]]);
                // Test only a boolean key actually read in this runtime.
                if ([trace[@"keys"] containsObject:@"liveRenderedBackground"]) {
                    NSMutableDictionary *liveMetadata = [metadata mutableCopy];
                    liveMetadata[@"liveRenderedBackground"] = @YES;
                    result[@"live_reference"] = ConstructSyntheticPass(passClass, liveMetadata, bundle, accessor);
                    result[@"live_observer"] = ConstructSyntheticPass(passClass, [[ShaderRecordingDictionary alloc] initWithStorage:liveMetadata], bundle, accessor);
                    result[@"live_parity"] = @((BOOL)[result[@"live_reference"] isEqual:result[@"live_observer"]]);
                }
            }
        } else result[@"reason"] = @"pass_control_not_established";
    } @catch (NSException *exception) { result[@"reason"] = @"pass_fixture_exception"; }
    return result;
}
