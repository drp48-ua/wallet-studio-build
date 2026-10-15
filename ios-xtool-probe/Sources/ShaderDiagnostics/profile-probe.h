// Only own synthetic metadata is used. No Wallet library or service is queried.
static NSDictionary *SyntheticProfileSnapshot(id profile, Class expected) {
    if (!profile || ![profile isKindOfClass:expected]) return @{};
    NSMutableDictionary *snapshot=[NSMutableDictionary dictionary];
    Class concrete=object_getClass(profile);
    for (NSString *name in @[@"type",@"passStyle",@"layoutMode"]) {
        SEL selector=NSSelectorFromString(name);
        if(!ScalarGetter(concrete,selector,@encode(long long))) return @{};
        long long (*getter)(id,SEL)=(void*)class_getMethodImplementation(concrete,selector);
        long long value=getter(profile,selector);
        if(value < -1 || value > 1024) return @{};
        snapshot[name]=@(value); // Framework-produced values; none assigned by this probe.
    }
    for (NSString *name in @[@"backgroundColor",@"foregroundColor",@"labelColor"]) {
        SEL selector=NSSelectorFromString(name);
        if(!ObjectMethod(concrete,selector,0)) return @{};
        id (*getter)(id,SEL)=(void*)class_getMethodImplementation(concrete,selector);
        snapshot[[name stringByAppendingString:@"Present"]]=@((BOOL)(getter(profile,selector)!=nil));
    }
    return snapshot;
}

static NSMutableDictionary *EmptySyntheticProfileResult(void) {
    return [@{@"status":@"unknown",@"reason":@"profile_control_unavailable",
        @"reference_present":@NO,@"constructed":@NO,@"parity":@NO,@"bound":@NO,
        @"metadata_lookup_keys":@[],@"trace_rejected":@NO,@"trace_truncated":@NO,
        @"reference_snapshot":@{},@"observer_snapshot":@{}} mutableCopy];
}

static BOOL BindSyntheticProfile(id pass, NSDictionary *metadata, NSURL *directory, NSMutableDictionary *result) {
    @try {
        Class profileClass=NSClassFromString(@"PKPassDisplayProfile");
        Class concrete=object_getClass(pass);
        SEL profileSelector=NSSelectorFromString(@"displayProfile");
        if(!profileClass || !ObjectMethod(concrete,profileSelector,0) ||
            !ObjectMethod(profileClass,@selector(initWithDictionary:bundle:),2)) return NO;
        id (*getProfile)(id,SEL)=(void*)class_getMethodImplementation(concrete,profileSelector);
        id before=getProfile(pass,profileSelector);
        result[@"reference_present"]=@((BOOL)(before!=nil));
        if(before) { result[@"reason"]=@"reference_profile_already_present"; return NO; }
        ShaderRecordingDictionary *observed=[[ShaderRecordingDictionary alloc] initWithStorage:metadata];
        if(![[observed objectForKey:@"formatVersion"] isEqual:metadata[@"formatVersion"]]) return NO;
        NSBundle *bundle=[NSBundle bundleWithURL:directory];
        id reference=[(id<NativePassInitializing>)[profileClass alloc] initWithDictionary:metadata bundle:bundle];
        id observer=[(id<NativePassInitializing>)[profileClass alloc] initWithDictionary:observed bundle:bundle];
        result[@"constructed"]=@((BOOL)(reference!=nil && observer!=nil));
        NSDictionary *a=SyntheticProfileSnapshot(reference,profileClass);
        NSDictionary *b=SyntheticProfileSnapshot(observer,profileClass);
        result[@"reference_snapshot"]=a; result[@"observer_snapshot"]=b;
        NSDictionary *trace=[observed trace];
        result[@"trace_rejected"]=trace[@"rejected"]; result[@"trace_truncated"]=trace[@"truncated"];
        BOOL parity=a.count==6 && [a isEqual:b] && ![trace[@"rejected"] boolValue] && ![trace[@"truncated"] boolValue];
        result[@"parity"]=@(parity);
        if(!parity) { result[@"reason"]=@"profile_construction_not_established"; return NO; }
        result[@"metadata_lookup_keys"]=trace[@"keys"];
        SEL setterSelector=NSSelectorFromString(@"setDisplayProfile:");
        Method setter=class_getInstanceMethod(concrete,setterSelector);
        char ret[64]={0},arg[64]={0};
        if(!setter || method_getNumberOfArguments(setter)!=3) return NO;
        method_getReturnType(setter,ret,sizeof(ret)); method_getArgumentType(setter,2,arg,sizeof(arg));
        if(ret[0]!='v' || arg[0]!='@') return NO;
        void (*put)(id,SEL,id)=(void*)method_getImplementation(setter);
        put(pass,setterSelector,reference);
        // PKPass declares copy semantics: verify native class and bounded properties,
        // rather than assuming the setter keeps the same object identity.
        id after=getProfile(pass,profileSelector);
        BOOL bound=[SyntheticProfileSnapshot(after,profileClass) isEqual:a];
        result[@"bound"]=@(bound);
        if(!bound) { result[@"reason"]=@"profile_binding_not_established"; return NO; }
        result[@"status"]=@"observed"; result[@"reason"]=@"synthetic_profile_bound";
        return YES;
    } @catch(NSException *exception) { result[@"reason"]=@"profile_exception"; return NO; }
}
