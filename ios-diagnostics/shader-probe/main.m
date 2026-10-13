#import <UIKit/UIKit.h>
#import <objc/runtime.h>
#import <dlfcn.h>
#import <TargetConditionals.h>
#import <string.h>
#import <stdlib.h>

// Only public runtime declarations are used. The observer is a subclass of the
// genuine PKDataAccessor; it supplies no card data, shaders, or archive objects.
@protocol NativeShaderInitializing <NSObject>
- (instancetype)initWithDataAccessor:(id)accessor suffix:(NSString *)suffix;
@end
@protocol NativeFileAccessorInitializing <NSObject>
- (instancetype)initWithFileURL:(NSURL *)url error:(NSError **)error;
@end

static NSMutableArray *requests;
static BOOL rejected, truncated;
static NSUInteger calls;
static NSMutableSet *rejectionKinds;
static BOOL ObjectMethod(Class cls, SEL selector, NSUInteger count);

static void Record(NSString *method, id name, id extension) {
    calls++;
    if (calls > 128) { truncated = YES; return; }
    Class descriptor = NSClassFromString(@"PKBundleResourceDescriptor");
    if (descriptor && [name isKindOfClass:descriptor]) {
        SEL nameSelector = NSSelectorFromString(@"name");
        SEL extensionSelector = NSSelectorFromString(@"extension");
        if (!ObjectMethod(descriptor, nameSelector, 0) || !ObjectMethod(descriptor, extensionSelector, 0)) {
            rejected = YES; [rejectionKinds addObject:@"nonstring_resource"]; return;
        }
        id (*getName)(id, SEL) = (void *)class_getMethodImplementation(descriptor, nameSelector);
        id (*getExtension)(id, SEL) = (void *)class_getMethodImplementation(descriptor, extensionSelector);
        id resourceObject = name;
        name = getName(resourceObject, nameSelector);
        extension = getExtension(resourceObject, extensionSelector);
    }
    NSMutableDictionary *item = [@{@"method": method} mutableCopy];
    for (NSArray *pair in @[@[@"name", name ?: NSNull.null], @[@"extension", extension ?: NSNull.null]]) {
        id value = pair[1];
        if (value == NSNull.null) { item[pair[0]] = NSNull.null; continue; }
        if (![value isKindOfClass:NSString.class]) { rejected = YES; [rejectionKinds addObject:@"nonstring_resource"]; continue; }
        NSString *text = value;
        NSRegularExpression *allowed = [NSRegularExpression regularExpressionWithPattern:@"\\A[A-Za-z_][A-Za-z0-9_.-]{0,95}\\z" options:0 error:nil];
        NSRegularExpression *identity = [NSRegularExpression regularExpressionWithPattern:@"[0-9a-fA-F]{24,}" options:0 error:nil];
        NSRange range = NSMakeRange(0, text.length);
        if (![allowed numberOfMatchesInString:text options:0 range:range] ||
            [identity numberOfMatchesInString:text options:0 range:range]) {
            rejected = YES;
            [rejectionKinds addObject:text.length == 0 ? @"empty_string" : [text containsString:@"/"] ? @"path_string" : @"other_string"];
            continue;
        }
        item[pair[0]] = text;
    }
    if (requests.count < 32) [requests addObject:item]; else truncated = YES;
}

static id Resource(id self, SEL selector, id name) {
    Record(@"resource", name, nil);
    return nil; // The reference accessor also has no data.
}
static id NamedResource(id self, SEL selector, id name, id extension) {
    Record(@"named_resource", name, extension);
    return nil;
}
static id Resources(id self, SEL selector, id names) {
    if (![names isKindOfClass:NSArray.class] && ![names isKindOfClass:NSSet.class]) {
        rejected = YES;
        [rejectionKinds addObject:names == nil ? @"nil_group" : [names isKindOfClass:NSSet.class] ? @"set_group" : [names isKindOfClass:NSDictionary.class] ? @"dictionary_group" : @"other_group"];
        return nil;
    }
    if ([names count] > 32) { truncated = YES; return nil; }
    NSArray *ordered = [names isKindOfClass:NSSet.class] ? [names allObjects] : names;
    BOOL stringsOnly = YES;
    for (id name in ordered) if (![name isKindOfClass:NSString.class]) stringsOnly = NO;
    if (stringsOnly) ordered = [ordered sortedArrayUsingSelector:@selector(compare:)];
    for (id name in ordered) Record(@"resources", name, nil);
    return nil;
}

static BOOL ObjectMethod(Class cls, SEL selector, NSUInteger count) {
    Method method = class_getInstanceMethod(cls, selector);
    if (!method || method_getNumberOfArguments(method) != count + 2) return NO;
    char result[64]; method_getReturnType(method, result, sizeof(result));
    if (result[0] != '@') return NO;
    for (NSUInteger i = 2; i < count + 2; ++i) {
        char argument[64]; method_getArgumentType(method, (unsigned int)i, argument, sizeof(argument));
        if (argument[0] != '@') return NO;
    }
    return YES;
}

static NSDictionary *Construct(Class shaders, id accessor) {
    NSMutableDictionary *result = [@{@"status": @"unknown", @"object_returned": @NO,
        @"exception": @NO, @"getter_lengths": @[]} mutableCopy];
    @try {
        // Empty suffix is a bounded synthetic argument, not a presumed Wallet
        // suffix. Identical argument on reference and observer controls.
        id object = [(id<NativeShaderInitializing>)[shaders alloc] initWithDataAccessor:accessor suffix:@""];
        result[@"object_returned"] = @((BOOL)(object != nil));
        if (!object) return result;
        NSMutableArray *lengths = [NSMutableArray array];
        for (NSString *name in @[@"fragmentEntryPointShader", @"geometryEntryPointShader",
                                @"lightingModelEntryPointShader", @"surfaceEntryPointShader"]) {
            SEL selector = NSSelectorFromString(name);
            if (!ObjectMethod(shaders, selector, 0)) return result;
            id (*getter)(id, SEL) = (void *)class_getMethodImplementation(shaders, selector);
            id value = getter(object, selector);
            if (value && ![value isKindOfClass:NSString.class]) return result;
            [lengths addObject:@(value ? [value length] : 0)];
        }
        result[@"getter_lengths"] = lengths;
        result[@"status"] = object ? @"observed" : @"unknown";
    } @catch (NSException *exception) {
        result[@"exception"] = @YES; // No exception description or private payload.
    }
    return result;
}

static NSDictionary *Probe(void) {
    NSMutableDictionary *result = [@{@"status": @"unknown", @"reason": @"runtime_unavailable",
        @"reference": @{}, @"observer": @{}, @"requests": @[], @"calibration": @NO,
        @"parity": @NO, @"trace_rejected": @NO, @"trace_truncated": @NO, @"rejection_kinds": @[]} mutableCopy];
    Class accessor = NSClassFromString(@"PKDataAccessor");
    Class shaders = NSClassFromString(@"PKLiveRenderedShaderSet");
    if (!accessor || !shaders) return result;
    NSArray *names = @[@"dataForBundleResource:", @"dataForBundleResourceNamed:withExtension:", @"dataForBundleResources:"];
    NSUInteger counts[] = {1, 2, 1};
    IMP implementations[] = {(IMP)Resource, (IMP)NamedResource, (IMP)Resources};
    if (!ObjectMethod(shaders, @selector(initWithDataAccessor:suffix:), 2)) {
        result[@"reason"] = @"signature_unavailable"; return result;
    }
    for (NSUInteger i = 0; i < names.count; ++i) {
        if (!ObjectMethod(accessor, NSSelectorFromString(names[i]), counts[i])) {
            result[@"reason"] = @"signature_unavailable"; return result;
        }
    }
    Class observer = objc_allocateClassPair(accessor, "WalletStudioShaderResourceObserverV1", 0);
    if (!observer) { result[@"reason"] = @"observer_unavailable"; return result; }
    for (NSUInteger i = 0; i < names.count; ++i) {
        SEL selector = NSSelectorFromString(names[i]);
        Method method = class_getInstanceMethod(accessor, selector);
        if (!class_addMethod(observer, selector, implementations[i], method_getTypeEncoding(method))) {
            objc_disposeClassPair(observer); result[@"reason"] = @"observer_unavailable"; return result;
        }
    }
    objc_registerClassPair(observer);
    @try {
        id referenceAccessor = [accessor new];
        id observedAccessor = [observer new];
        if (!referenceAccessor || !observedAccessor) {
            result[@"reason"] = @"accessor_unavailable"; return result;
        }
        requests = [NSMutableArray array]; rejectionKinds = [NSMutableSet set]; calls = 0; rejected = truncated = NO;
        id (*dispatchResource)(id, SEL, id) = (void *)class_getMethodImplementation(observer, NSSelectorFromString(names[0]));
        id observedControl = dispatchResource(observedAccessor, NSSelectorFromString(names[0]), @"lab-control.shader");
        id (*referenceResource)(id, SEL, id) = (void *)class_getMethodImplementation(accessor, NSSelectorFromString(names[0]));
        id referenceControl = referenceResource(referenceAccessor, NSSelectorFromString(names[0]), @"lab-control.shader");
        result[@"calibration"] = @((BOOL)(observedControl == nil && referenceControl == nil && requests.count == 1 && !rejected && !truncated));
        requests = [NSMutableArray array]; rejectionKinds = [NSMutableSet set]; calls = 0; rejected = truncated = NO;
        result[@"reference"] = Construct(shaders, referenceAccessor);
        result[@"observer"] = Construct(shaders, observedAccessor);
        result[@"parity"] = @((BOOL)[result[@"reference"] isEqual:result[@"observer"]]);
        result[@"trace_rejected"] = @(rejected);
        result[@"trace_truncated"] = @(truncated);
        result[@"rejection_kinds"] = [[rejectionKinds allObjects] sortedArrayUsingSelector:@selector(compare:)];
        // Export names only with an observed, calibrated, equivalent outcome.
        BOOL usable = [result[@"calibration"] boolValue] && [result[@"parity"] boolValue] &&
            [result[@"reference"][@"status"] isEqual:@"observed"] && !rejected && !truncated;
        if (usable) {
            result[@"requests"] = [requests copy]; result[@"status"] = @"observed";
            result[@"reason"] = calls ? @"native_resource_requests_observed" : @"no_resource_requests_observed";
        } else result[@"reason"] = @"control_not_established";
    } @catch (NSException *exception) { result[@"reason"] = @"accessor_exception"; }
    return result;
}

static NSDictionary *ProbeSyntheticFileLoading(NSDictionary *discovery) {
    NSMutableDictionary *result = [@{@"status": @"unknown", @"reason": @"discovery_unavailable",
        @"native_accessor_created": @NO, @"source_preserved": @NO} mutableCopy];
    if (![discovery[@"status"] isEqual:@"observed"]) return result;
    NSSet *expected = [NSSet setWithArray:@[@"fragment", @"surface", @"geometry", @"lighting"]];
    NSMutableSet *observed = [NSMutableSet set];
    for (NSDictionary *item in discovery[@"requests"]) {
        if (![item[@"extension"] isEqual:@"metal"] || ![expected containsObject:item[@"name"]]) return result;
        [observed addObject:item[@"name"]];
    }
    if (![observed isEqual:expected]) return result;
    Class accessorClass = NSClassFromString(@"PKFileDataAccessor");
    Class shaderClass = NSClassFromString(@"PKLiveRenderedShaderSet");
    SEL initializer = @selector(initWithFileURL:error:);
    Method method = class_getInstanceMethod(accessorClass, initializer);
    char returnType[64] = {0}, first[64] = {0}, second[64] = {0};
    if (!method || method_getNumberOfArguments(method) != 4) {
        result[@"reason"] = @"file_accessor_signature_unavailable"; return result;
    }
    method_getReturnType(method, returnType, sizeof(returnType));
    method_getArgumentType(method, 2, first, sizeof(first));
    method_getArgumentType(method, 3, second, sizeof(second));
    if (returnType[0] != '@' || first[0] != '@' || strncmp(second, "^@", 2)) {
        result[@"reason"] = @"file_accessor_signature_unavailable"; return result;
    }
    NSURL *root = [[NSFileManager defaultManager] URLsForDirectory:NSCachesDirectory inDomains:NSUserDomainMask].firstObject;
    NSURL *directory = [root URLByAppendingPathComponent:@"WalletStudioSyntheticShaders.pkpass" isDirectory:YES];
    NSString *source = @"// Wallet Studio synthetic loader control\n";
    @try {
        NSError *error = nil;
        if (![[NSFileManager defaultManager] createDirectoryAtURL:directory withIntermediateDirectories:YES attributes:nil error:&error]) {
            result[@"reason"] = @"synthetic_fixture_unavailable"; return result;
        }
        NSData *metadata = [@"{}" dataUsingEncoding:NSUTF8StringEncoding];
        if (![metadata writeToURL:[directory URLByAppendingPathComponent:@"pass.json"] atomically:YES]) {
            result[@"reason"] = @"synthetic_fixture_unavailable"; return result;
        }
        for (NSString *name in expected) {
            if (![[source dataUsingEncoding:NSUTF8StringEncoding] writeToURL:[directory URLByAppendingPathComponent:[name stringByAppendingPathExtension:@"metal"]] atomically:YES]) {
                result[@"reason"] = @"synthetic_fixture_unavailable"; return result;
            }
        }
        id accessor = [(id<NativeFileAccessorInitializing>)[accessorClass alloc] initWithFileURL:directory error:&error];
        result[@"native_accessor_created"] = @((BOOL)(accessor != nil));
        if (!accessor) { result[@"reason"] = @"file_accessor_rejected"; return result; }
        id shaders = [(id<NativeShaderInitializing>)[shaderClass alloc] initWithDataAccessor:accessor suffix:@""];
        if (!shaders) { result[@"reason"] = @"shader_set_rejected"; return result; }
        BOOL allEqual = YES;
        for (NSString *name in @[@"fragmentEntryPointShader", @"surfaceEntryPointShader", @"geometryEntryPointShader", @"lightingModelEntryPointShader"]) {
            SEL selector = NSSelectorFromString(name);
            if (!ObjectMethod(shaderClass, selector, 0)) { allEqual = NO; break; }
            id (*getter)(id, SEL) = (void *)class_getMethodImplementation(shaderClass, selector);
            id value = getter(shaders, selector);
            if (![value isKindOfClass:NSString.class] || ![value isEqual:source]) allEqual = NO;
        }
        result[@"status"] = @"observed";
        result[@"source_preserved"] = @(allEqual);
        result[@"reason"] = allEqual ? @"synthetic_source_preserved" : @"synthetic_source_not_preserved";
    } @catch (NSException *exception) { result[@"reason"] = @"file_accessor_exception"; }
    return result;
}

#import "pass-probe.h"
#import "material-probe.h"
#import "renderer-probe.h"

@interface ShaderDelegate : UIResponder <UIApplicationDelegate>
@property (strong, nonatomic) UIWindow *window;
@end
@implementation ShaderDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *view = [UIViewController new];
    view.view.backgroundColor = UIColor.systemBackgroundColor;
    self.window.rootViewController = view; [self.window makeKeyAndVisible];
    UILabel *label = [UILabel new]; label.numberOfLines = 0;
    label.text = @"Shaders v11 · preparando diagnóstico";
    label.translatesAutoresizingMaskIntoConstraints = NO;
    [view.view addSubview:label];
    [NSLayoutConstraint activateConstraints:@[
        [label.topAnchor constraintEqualToAnchor:view.view.safeAreaLayoutGuide.topAnchor constant:20],
        [label.leadingAnchor constraintEqualToAnchor:view.view.leadingAnchor constant:16],
        [label.trailingAnchor constraintEqualToAnchor:view.view.trailingAnchor constant:-16]]];
    dispatch_async(dispatch_get_main_queue(), ^{
        NSString *framework = @"/System/Library/PrivateFrameworks/PassKitCore.framework/PassKitCore";
        BOOL loaded = dlopen(framework.UTF8String, RTLD_NOW | RTLD_LOCAL) != NULL;
        dlopen("/System/Library/PrivateFrameworks/PassKitUI.framework/PassKitUI", RTLD_NOW | RTLD_LOCAL);
        NSDictionary *discovery = Probe();
        NSDictionary *loader = ProbeSyntheticFileLoading(discovery);
        NSDictionary *passProbe = ProbeSyntheticPass(loader);
        NSMutableDictionary *rendererProbe = ProbeSyntheticRenderer(passProbe,view.view);
        NSDictionary *report = @{@"schema": @1, @"revision": @"shader_resources_v11",
            @"scope": @"synthetic_accessors_and_own_files_only", @"Wallet_animation_proven": @NO,
            @"runtime_iOS": UIDevice.currentDevice.systemVersion,
            @"environment": TARGET_OS_SIMULATOR ? @"simulator" : @"device_app_own_sandbox",
            @"framework_loaded": @(loaded), @"probe": discovery,
            @"file_loader": loader, @"pass_probe": passProbe, @"renderer_probe": rendererProbe};
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW,4*NSEC_PER_SEC),dispatch_get_main_queue(), ^{
        NSURL *directory = [[NSFileManager defaultManager] URLsForDirectory:NSDocumentDirectory inDomains:NSUserDomainMask].firstObject;
        NSData *data = [NSJSONSerialization dataWithJSONObject:report options:NSJSONWritingPrettyPrinted error:nil];
        BOOL saved = [data writeToURL:[directory URLByAppendingPathComponent:@"shader-report.json"] atomically:YES];
        label.text = saved ? @"Shaders v11 · diagnóstico guardado\nNo acredita animación en Wallet." : @"Shaders v11 · informe no guardado";
        });
    });
    return YES;
}
@end
int main(int argc, char *argv[]) {
    @autoreleasepool { return UIApplicationMain(argc, argv, nil, NSStringFromClass(ShaderDelegate.class)); }
}
