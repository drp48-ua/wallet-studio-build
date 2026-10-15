@protocol NativeCardViewInitializing <NSObject>
- (instancetype)initWithFrame:(CGRect)frame pass:(id)pass;
@end

#import "admission-probe.h"
#import "profile-probe.h"
#import "material-input-probe.h"

static NSArray *ReadSyntheticCenter(UIView *view) {
    UIGraphicsImageRenderer *renderer = [[UIGraphicsImageRenderer alloc] initWithSize:view.bounds.size];
    UIImage *image = [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {
        [view drawViewHierarchyInRect:view.bounds afterScreenUpdates:YES];
    }];
    CGImageRef cg = image.CGImage;
    if (!cg) return @[];
    CGRect center = CGRectMake(CGImageGetWidth(cg)/2, CGImageGetHeight(cg)/2, 1, 1);
    CGImageRef pixel = CGImageCreateWithImageInRect(cg, center);
    if (!pixel) return @[];
    unsigned char bytes[4] = {0};
    CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
    CGContextRef bitmap = CGBitmapContextCreate(bytes, 1, 1, 8, 4, space, kCGImageAlphaPremultipliedLast);
    if (bitmap) CGContextDrawImage(bitmap, CGRectMake(0,0,1,1), pixel);
    if (bitmap) CGContextRelease(bitmap);
    CGColorSpaceRelease(space); CGImageRelease(pixel);
    return bitmap ? @[@(bytes[0]), @(bytes[1]), @(bytes[2])] : @[];
}

static void GatherSyntheticViewKinds(UIView *view, NSMutableSet *kinds, NSUInteger depth) {
    if (depth > 8) return;
    for (NSString *name in @[@"SCNView", @"MTKView", @"PKTexturedCardView", @"PKPayLaterCardView", @"UIImageView"]) {
        Class cls = NSClassFromString(name);
        if (cls && [view isKindOfClass:cls]) [kinds addObject:name];
    }
    for (UIView *child in view.subviews) GatherSyntheticViewKinds(child, kinds, depth+1);
}

static NSMutableDictionary *ProbeSyntheticRenderer(NSDictionary *passProbe, UIView *host) {
    NSMutableDictionary *result = [@{@"status": @"unknown", @"reason": @"pass_control_unavailable",
        @"view_created": @NO, @"shader_source_exact": @NO, @"capture_control": @NO,
        @"synthetic_model": @"PKPaymentPass",
        @"material_bound": @NO, @"native_image_set_type": @0,
        @"capture_control_rgb": @[],
        @"capture_state": @{@"application_active":@((BOOL)(UIApplication.sharedApplication.applicationState==UIApplicationStateActive)),
            @"host_in_window":@((BOOL)(host.window!=nil))},
        @"initializer_names": @[],
        @"admission_probe": @{@"status":@"unknown",@"getter_names":@[],@"trace_truncated":@NO,@"constructor_nil_parity":@NO},
        @"profile_probe": EmptySyntheticProfileResult(),
        @"material_inputs": [@{@"baseline_view_nil":@NO,@"complete_material_bound":@NO,@"image_ref_count":@0,@"candidate_view_created":@NO} mutableCopy],
        @"view_kinds": @[], @"samples_rgb": @[], @"three_phases_observed": @NO} mutableCopy];
    NSDictionary *control = passProbe[@"live_reference"];
    if (![passProbe[@"live_parity"] boolValue] || ![control[@"shader_source_preserved"] boolValue] ||
        ![control[@"live_background"] boolValue] || [control[@"live_render_type"] unsignedIntegerValue] != 1) return result;
    Class viewClass = NSClassFromString(@"PKLiveRenderedCardFaceView");
    if (!viewClass || ![viewClass isSubclassOfClass:UIView.class]) {
        result[@"reason"] = @"renderer_class_unavailable"; return result;
    }
    unsigned int methodCount=0;
    Method *methods=class_copyMethodList(viewClass,&methodCount);
    NSMutableArray *initializers=[NSMutableArray array];
    for(unsigned int i=0;i<methodCount && initializers.count<32;++i){
        NSString *name=NSStringFromSelector(method_getName(methods[i]));
        if(![name hasPrefix:@"init"] || name.length>128) continue;
        NSCharacterSet *allowed=[NSCharacterSet characterSetWithCharactersInString:@"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_:"];
        if([name rangeOfCharacterFromSet:allowed.invertedSet].location==NSNotFound) [initializers addObject:name];
    }
    free(methods); result[@"initializer_names"]=[initializers sortedArrayUsingSelector:@selector(compare:)];
    Method init = class_getInstanceMethod(viewClass, @selector(initWithFrame:pass:));
    char ret[128] = {0}, frameType[128] = {0}, passType[128] = {0};
    if (!init || method_getNumberOfArguments(init) != 4) {
        result[@"reason"] = @"renderer_signature_unavailable"; return result;
    }
    method_getReturnType(init, ret, sizeof(ret));
    method_getArgumentType(init, 2, frameType, sizeof(frameType));
    method_getArgumentType(init, 3, passType, sizeof(passType));
    if (ret[0] != '@' || strcmp(frameType,@encode(CGRect)) || passType[0] != '@') {
        result[@"reason"] = @"renderer_signature_unavailable"; return result;
    }
    @try {
        UIView *captureControl = [[UIView alloc] initWithFrame:CGRectMake(0,0,20,20)];
        captureControl.backgroundColor = UIColor.redColor;
        [host addSubview:captureControl]; [captureControl layoutIfNeeded];
        NSArray *controlRGB = ReadSyntheticCenter(captureControl);
        result[@"capture_control_rgb"]=controlRGB;
        [captureControl removeFromSuperview];
        BOOL captured = controlRGB.count == 3 && [controlRGB[0] intValue]>220 &&
            [controlRGB[1] intValue]<30 && [controlRGB[2] intValue]<30;
        result[@"capture_control"] = @(captured);
        if (!captured) { result[@"reason"] = @"capture_control_failed"; return result; }
        NSURL *root = [[NSFileManager defaultManager] URLsForDirectory:NSCachesDirectory inDomains:NSUserDomainMask].firstObject;
        NSURL *directory = [root URLByAppendingPathComponent:@"WalletStudioSyntheticShaders.pkpass" isDirectory:YES];
        // Own modifier uses SceneKit's documented u_time and fragment output.
        // Compatibility with this Wallet renderer is precisely the question.
        NSString *source = @"#pragma body\nfloat phase = mod(floor(u_time / 0.8), 3.0);\n_output.color = phase < 1.0 ? vec4(1.0,0.0,0.0,1.0) : (phase < 2.0 ? vec4(0.0,1.0,0.0,1.0) : vec4(0.0,0.0,1.0,1.0));\n";
        if (![[source dataUsingEncoding:NSUTF8StringEncoding] writeToURL:[directory URLByAppendingPathComponent:@"fragment.metal"] atomically:YES]) {
            result[@"reason"] = @"renderer_fixture_unavailable"; return result;
        }
        Class accessorClass = NSClassFromString(@"PKFileDataAccessor");
        NSError *error = nil;
        NSDictionary *metadata = @{@"formatVersion": @1, @"passTypeIdentifier": @"pass.local.walletstudio.synthetic",
            @"serialNumber": @"synthetic-only", @"teamIdentifier": @"ABCDEFGHIJ", @"organizationName": @"Wallet Studio",
            @"description": @"Synthetic shader diagnostic", @"generic": @{@"primaryFields": @[]}, @"liveRenderedBackground": @YES};
        NSData *metadataData = [NSJSONSerialization dataWithJSONObject:metadata options:0 error:nil];
        if (![metadataData writeToURL:[directory URLByAppendingPathComponent:@"pass.json"] atomically:YES]) {
            result[@"reason"] = @"renderer_fixture_unavailable"; return result;
        }
        id accessor = [(id<NativeFileAccessorInitializing>)[accessorClass alloc] initWithFileURL:directory error:&error];
        Class passClass = NSClassFromString(@"PKPaymentPass");
        if (!passClass || !ObjectMethod(passClass,@selector(initWithDictionary:bundle:),2)) {
            result[@"reason"] = @"renderer_fixture_unavailable"; return result;
        }
        id pass = [(id<NativePassInitializing>)[passClass alloc] initWithDictionary:metadata bundle:[NSBundle bundleWithURL:directory]];
        if (!accessor || !pass || ![pass isKindOfClass:passClass]) { result[@"reason"] = @"renderer_fixture_unavailable"; return result; }
        SEL bind = NSSelectorFromString(@"setDataAccessor:");
        Method bindMethod = class_getInstanceMethod(object_getClass(pass),bind);
        char bindReturn[64]={0}, bindArgument[64]={0};
        if (!bindMethod || method_getNumberOfArguments(bindMethod)!=3) {
            result[@"reason"]=@"renderer_fixture_unavailable"; return result;
        }
        method_getReturnType(bindMethod,bindReturn,sizeof(bindReturn));
        method_getArgumentType(bindMethod,2,bindArgument,sizeof(bindArgument));
        if (bindReturn[0]!='v' || bindArgument[0]!='@') { result[@"reason"]=@"renderer_fixture_unavailable"; return result; }
        void (*bindAccessor)(id, SEL, id) = (void *)method_getImplementation(bindMethod);
        bindAccessor(pass,bind,accessor);
        SEL typeSelector = NSSelectorFromString(@"liveRenderType");
        if (!ScalarGetter(object_getClass(pass),typeSelector,@encode(NSUInteger))) {
            result[@"reason"] = @"renderer_fixture_unavailable"; return result;
        }
        NSUInteger (*getType)(id,SEL) = (void *)class_getMethodImplementation(object_getClass(pass),typeSelector);
        if (getType(pass,typeSelector)!=1) { result[@"reason"]=@"renderer_fixture_unavailable"; return result; }
        SEL setSelector = NSSelectorFromString(@"liveRenderedShaderSet");
        id (*getSet)(id, SEL) = (void *)class_getMethodImplementation(object_getClass(pass),setSelector);
        id shaderSet = getSet(pass,setSelector);
        SEL fragment = NSSelectorFromString(@"fragmentEntryPointShader");
        if (!shaderSet || !ObjectMethod(object_getClass(shaderSet),fragment,0)) {
            result[@"reason"] = @"renderer_fixture_unavailable"; return result;
        }
        id (*getSource)(id,SEL) = (void *)class_getMethodImplementation(object_getClass(shaderSet),fragment);
        result[@"shader_source_exact"] = @((BOOL)[getSource(shaderSet,fragment) isEqual:source]);
        if (![result[@"shader_source_exact"] boolValue]) { result[@"reason"] = @"renderer_fixture_unavailable"; return result; }
        if (!BindSyntheticDiffuse(pass,result)) { result[@"reason"]=@"renderer_fixture_unavailable"; return result; }
        UIView *view = (UIView *)[(id<NativeCardViewInitializing>)[viewClass alloc] initWithFrame:CGRectMake(16,100,320,202) pass:pass];
        if (!view) {
            result[@"admission_probe"]=ProbeSyntheticAdmission(passClass,viewClass,metadata,directory,accessor,source);
            NSMutableDictionary *inputs=result[@"material_inputs"];
            inputs[@"baseline_view_nil"]=@YES;
            if(CompleteOwnMaterial(pass,inputs)) {
                view=(UIView*)[(id<NativeCardViewInitializing>)[viewClass alloc] initWithFrame:CGRectMake(16,100,320,202) pass:pass];
                inputs[@"candidate_view_created"]=@((BOOL)(view!=nil));
            }
            if(!view) {
            NSMutableDictionary *profile=result[@"profile_probe"];
            if(!BindSyntheticProfile(pass,metadata,directory,profile)) { result[@"reason"]=@"renderer_view_unavailable"; return result; }
            // A profile setter may invalidate cached assets. Re-establish final controls.
            shaderSet=getSet(pass,setSelector);
            if(getType(pass,typeSelector)!=1 || !shaderSet || !ObjectMethod(object_getClass(shaderSet),fragment,0)) {
                result[@"reason"]=@"renderer_fixture_unavailable"; return result;
            }
            getSource=(void*)class_getMethodImplementation(object_getClass(shaderSet),fragment);
            result[@"shader_source_exact"]=@((BOOL)[getSource(shaderSet,fragment) isEqual:source]);
            if(![result[@"shader_source_exact"] boolValue] || !BindSyntheticDiffuse(pass,result)) {
                result[@"reason"]=@"renderer_fixture_unavailable"; return result;
            }
            view=(UIView*)[(id<NativeCardViewInitializing>)[viewClass alloc] initWithFrame:CGRectMake(16,100,320,202) pass:pass];
            if(!view) { result[@"reason"]=@"renderer_view_unavailable"; return result; }
        }
        }
        [host addSubview:view]; [view layoutIfNeeded];
        result[@"view_created"] = @YES;
        SEL present = NSSelectorFromString(@"didPresent");
        Method presentMethod = class_getInstanceMethod(viewClass,present);
        if (presentMethod && method_getNumberOfArguments(presentMethod)==2) {
            char type[32]; method_getReturnType(presentMethod,type,sizeof(type));
            if (type[0]=='v') { void (*didPresent)(id,SEL) = (void *)method_getImplementation(presentMethod); didPresent(view,present); }
        }
        NSMutableSet *kinds = [NSMutableSet set]; GatherSyntheticViewKinds(view,kinds,0);
        result[@"view_kinds"] = [[kinds allObjects] sortedArrayUsingSelector:@selector(compare:)];
        NSMutableArray *samples = [NSMutableArray array]; result[@"samples_rgb"] = samples;
        for (NSUInteger i=0; i<6; ++i) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW,(int64_t)((0.25+0.45*i)*NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
                @try {
                    NSArray *rgb = ReadSyntheticCenter(view);
                    [samples addObject:rgb];
                    if (i==5) {
                        BOOL phases[3]={NO,NO,NO};
                        for (NSArray *sample in samples) if (sample.count==3) for (NSUInteger phase=0;phase<3;++phase) {
                            BOOL dominant=[sample[phase] intValue]>200;
                            for(NSUInteger channel=0;channel<3;++channel) if(channel!=phase && [sample[channel] intValue]>50) dominant=NO;
                            phases[phase]|=dominant;
                        }
                        BOOL all = phases[0] && phases[1] && phases[2];
                        result[@"three_phases_observed"]=@(all);
                        result[@"status"]=@"observed";
                        result[@"reason"]=all ? @"synthetic_renderer_three_phases" : @"synthetic_renderer_no_three_phases";
                    }
                } @catch(NSException *exception) { result[@"reason"]=@"renderer_capture_exception"; }
            });
        }
    } @catch(NSException *exception) { result[@"reason"]=@"renderer_exception"; }
    return result;
}
