// Observe only the own synthetic model; original getter implementations remain intact.
static NSDictionary *ProbeSyntheticAdmission(Class passClass, Class viewClass, NSDictionary *metadata, NSURL *directory, id accessor, NSString *source) {
    NSMutableDictionary *result=[@{@"status":@"unknown",@"getter_names":@[],@"trace_truncated":@NO,@"constructor_nil_parity":@NO} mutableCopy];
    Class observer=objc_allocateClassPair(passClass,"WalletStudioAdmissionObserver",0);
    if(!observer) return result;
    NSMutableArray *names=[NSMutableArray array]; __block BOOL active=NO; __block BOOL truncated=NO;
    for(NSString *name in @[@"liveRenderType",@"homeKeyLiveRenderType",@"liveRenderedBackground",@"liveRenderedShaderSet",@"liveRenderedImageSet",@"liveRenderedArmedStateImageSet",@"displayProfile",@"passType"]) {
        SEL sel=NSSelectorFromString(name); Method method=class_getInstanceMethod(passClass,sel);
        if(!method || method_getNumberOfArguments(method)!=2) continue;
        char ret[128]={0}; method_getReturnType(method,ret,sizeof(ret)); IMP original=method_getImplementation(method);
        void (^record)(void)=^{ if(active && ![names containsObject:name]) { if(names.count<16) [names addObject:name]; else truncated=YES; } };
        IMP replacement=NULL;
        if(ret[0]=='@') replacement=imp_implementationWithBlock(^id(id self){ record(); id (*get)(id,SEL)=(void*)original; return get(self,sel); });
        else if(!strcmp(ret,@encode(bool))) replacement=imp_implementationWithBlock(^bool(id self){ record(); bool (*get)(id,SEL)=(void*)original; return get(self,sel); });
        else if(!strcmp(ret,@encode(unsigned long long))) replacement=imp_implementationWithBlock(^unsigned long long(id self){ record(); unsigned long long (*get)(id,SEL)=(void*)original; return get(self,sel); });
        else if(!strcmp(ret,@encode(long long))) replacement=imp_implementationWithBlock(^long long(id self){ record(); long long (*get)(id,SEL)=(void*)original; return get(self,sel); });
        if(replacement) class_addMethod(observer,sel,replacement,method_getTypeEncoding(method));
    }
    objc_registerClassPair(observer);
    @try {
        id pass=[(id<NativePassInitializing>)[observer alloc] initWithDictionary:metadata bundle:[NSBundle bundleWithURL:directory]];
        if(!pass) return result;
        SEL bind=NSSelectorFromString(@"setDataAccessor:");
        // Concrete signature was checked by the reference renderer fixture.
        void (*put)(id,SEL,id)=(void*)class_getMethodImplementation(passClass,bind); put(pass,bind,accessor);
        NSMutableDictionary *material=[NSMutableDictionary dictionary];
        if(!BindSyntheticDiffuse(pass,material)) return result;
        SEL type=NSSelectorFromString(@"liveRenderType");
        NSUInteger (*getType)(id,SEL)=(void*)class_getMethodImplementation(observer,type);
        if(getType(pass,type)!=1) return result;
        SEL setSelector=NSSelectorFromString(@"liveRenderedShaderSet");
        if(!ObjectMethod(observer,setSelector,0)) return result;
        id (*getSet)(id,SEL)=(void*)class_getMethodImplementation(observer,setSelector);
        id set=getSet(pass,setSelector); SEL fragment=NSSelectorFromString(@"fragmentEntryPointShader");
        if(!set || !ObjectMethod(object_getClass(set),fragment,0)) return result;
        id (*getSource)(id,SEL)=(void*)class_getMethodImplementation(object_getClass(set),fragment);
        if(![getSource(set,fragment) isEqual:source]) return result;
        // Calibrate dispatch before the constructor trace; release no getter values.
        active=YES; (void)getType(pass,type); active=NO;
        if(![names containsObject:@"liveRenderType"]) return result;
        [names removeAllObjects]; active=YES;
        UIView *view=(UIView*)[(id<NativeCardViewInitializing>)[viewClass alloc] initWithFrame:CGRectMake(16,100,320,202) pass:pass];
        active=NO;
        result[@"constructor_nil_parity"]=@((BOOL)(view==nil));
        result[@"trace_truncated"]=@(truncated);
        if(!view && !truncated) { result[@"status"]=@"observed"; result[@"getter_names"]=[names copy]; }
        // Nil parity is limited: subclass identity could itself influence the constructor.
    } @catch(NSException *exception) { active=NO; }
    return result;
}
