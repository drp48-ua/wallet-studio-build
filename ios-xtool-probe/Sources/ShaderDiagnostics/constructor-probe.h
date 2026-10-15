// Inspect only the loaded constructor implementation in this app's own process.
// No other task, user data, Wallet process or executable modification is involved.
#import <mach/mach.h>
#import <ptrauth.h>

static NSString *OwnTextureImage(void) {
    const char *name=class_getImageName(NSClassFromString(@"PKTexturedCardView"));
    if(!name) return nil;
    NSString *image=@(name);
    if(image.length>512 || ![image hasPrefix:@"/System/Library/"]) return nil;
    return image;
}
static BOOL OwnRenderImageAllowed(const char *path) {
    if(!path) return NO;
    if(!strcmp(path,"/System/Library/PrivateFrameworks/PassKitUI.framework/PassKitUI") ||
       !strcmp(path,"/System/Library/PrivateFrameworks/PassKitCore.framework/PassKitCore")) return YES;
    NSString *textureImage=OwnTextureImage();
    return textureImage && !strcmp(path,textureImage.UTF8String);
}

static BOOL ReadOwnCode(const void *source, void *destination, vm_size_t length) {
    vm_size_t read=0;
    return vm_read_overwrite(mach_task_self(),(vm_address_t)(uintptr_t)source,length,
        (vm_address_t)(uintptr_t)destination,&read)==KERN_SUCCESS && read==length;
}

// Resolve only standard ARM64 Objective-C selector stubs reached by direct BL.
// Selector names are runtime code metadata, never arguments or pass properties.
static NSArray *OwnSelectorCalls(const void *code, const unsigned char *bytes, NSUInteger length) {
    NSMutableArray *calls=[NSMutableArray array];
    for(NSUInteger offset=0;offset+4<=length && calls.count<128;offset+=4) {
        uint32_t word=0; memcpy(&word,bytes+offset,4);
        if((word&0xfc000000)!=0x94000000) continue;
        int64_t delta=word&0x03ffffff;
        if(delta&(1<<25)) delta-=1LL<<26;
        uintptr_t target=(uintptr_t)code+offset+delta*4;
        Dl_info image={0};
        if(!dladdr((const void*)target,&image) || !OwnRenderImageAllowed(image.dli_fname)) continue;
        uint32_t stub[2]={0};
        if(!ReadOwnCode((const void*)target,stub,sizeof(stub))) continue;
        if((stub[0]&0x9f00001f)!=0x90000001 || (stub[1]&0xffc003ff)!=0xf9400021) continue;
        int64_t pageDelta=((stub[0]>>5)&0x7ffff)*4+((stub[0]>>29)&3);
        if(pageDelta&(1<<20)) pageDelta-=1LL<<21;
        uintptr_t slot=(target&~(uintptr_t)0xfff)+pageDelta*4096+((stub[1]>>10)&0xfff)*8;
        SEL selector=NULL;
        if(!ReadOwnCode((const void*)slot,&selector,sizeof(selector)) || !sel_isMapped(selector)) continue;
        const char *name=sel_getName(selector);
        size_t count=name?strnlen(name,129):0;
        if(!count || count>128) continue;
        BOOL safe=YES;
        for(size_t i=0;i<count;++i) if(!((name[i]>='A'&&name[i]<='Z') ||
            (name[i]>='a'&&name[i]<='z') || (name[i]>='0'&&name[i]<='9') || name[i]=='_' || name[i]==':')) safe=NO;
        if(safe) [calls addObject:@{@"offset":@(offset),@"selector":@(name)}];
    }
    return calls;
}

static NSDictionary *ProbeOwnCodeForMethod(Method method) {
    NSMutableDictionary *result=[@{@"status":@"unknown",@"reason":@"method_unavailable",
        @"image_origin_verified":@NO,@"code_hex":@"",@"selector_calls":@[]} mutableCopy];
    if(!method) return result;
    const void *code=(const void*)ptrauth_strip(method_getImplementation(method),ptrauth_key_function_pointer);
    Dl_info image={0};
    if(!dladdr(code,&image) || !OwnRenderImageAllowed(image.dli_fname)) {
        result[@"reason"]=@"image_origin_unverified"; return result;
    }
    result[@"image_origin_verified"]=@YES;
    unsigned char bytes[4096]={0};
    if(!ReadOwnCode(code,bytes,sizeof(bytes))) {
        result[@"reason"]=@"own_code_read_unavailable"; return result;
    }
    NSMutableString *hex=[NSMutableString stringWithCapacity:sizeof(bytes)*2];
    for(NSUInteger i=0;i<sizeof(bytes);++i) [hex appendFormat:@"%02x",bytes[i]];
    result[@"status"]=@"observed"; result[@"reason"]=@"own_constructor_code_observed";
    result[@"code_hex"]=hex;
    result[@"selector_calls"]=OwnSelectorCalls(code,bytes,sizeof(bytes));
    return result;
}

static NSDictionary *ProbeOwnConstructor(void) {
    Method method=class_getInstanceMethod(NSClassFromString(@"PKLiveRenderedCardFaceView"),@selector(initWithFrame:pass:));
    char ret[128]={0},frame[128]={0},pass[128]={0};
    if(!method || method_getNumberOfArguments(method)!=4) return ProbeOwnCodeForMethod(NULL);
    method_getReturnType(method,ret,sizeof(ret)); method_getArgumentType(method,2,frame,sizeof(frame));
    method_getArgumentType(method,3,pass,sizeof(pass));
    if(ret[0]!='@' || strcmp(frame,@encode(CGRect)) || pass[0]!='@') return ProbeOwnCodeForMethod(NULL);
    return ProbeOwnCodeForMethod(method);
}

static NSArray *OwnPassKitClassNames(BOOL *truncated) {
    NSMutableArray *result=[NSMutableArray array];
    NSMutableSet *images=[NSMutableSet setWithArray:@[@"/System/Library/PrivateFrameworks/PassKitUI.framework/PassKitUI",
                            @"/System/Library/PrivateFrameworks/PassKitCore.framework/PassKitCore"]];
    NSString *textureImage=OwnTextureImage();if(textureImage) [images addObject:textureImage];
    for(NSString *image in [[images allObjects] sortedArrayUsingSelector:@selector(compare:)]) {
        unsigned int count=0;const char **names=objc_copyClassNamesForImage(image.UTF8String,&count);
        if(count>10000) { *truncated=YES;free(names);continue; }
        for(unsigned int i=0;i<count;++i) if(names[i]) [result addObject:@(names[i])];
        free(names);
    }
    return [result sortedArrayUsingSelector:@selector(compare:)];
}

static Class OwnMaterialConstructorClass(NSMutableArray *candidates) {
    BOOL truncated=NO;NSArray *names=OwnPassKitClassNames(&truncated);Class found=Nil;unsigned int matches=0;
    if(truncated) return Nil;
    SEL selector=NSSelectorFromString(@"initWithDiffuse:metalness:normal:");
    for(NSString *name in names) {
        Class cls=objc_lookUpClass(name.UTF8String);
        if(!cls || !class_getInstanceMethod(cls,selector)) continue;
        unsigned int amount=0;Method *methods=class_copyMethodList(cls,&amount);BOOL own=NO;
        for(unsigned int j=0;j<amount;++j) if(method_getName(methods[j])==selector) own=YES;
        free(methods);
        if(own) { ++matches;found=cls;if(candidates.count<8) [candidates addObject:name]; }
    }
    return matches==1?found:Nil;
}

static NSDictionary *OwnRendererCatalog(void) {
    BOOL truncated=NO;NSArray *classNames=OwnPassKitClassNames(&truncated);
    NSMutableArray *catalog=[NSMutableArray array];
    NSCharacterSet *allowed=[NSCharacterSet characterSetWithCharactersInString:@"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_:"];
    NSMutableCharacterSet *classAllowed=[allowed mutableCopy];[classAllowed addCharactersInString:@"."];
    for(NSString *name in classNames) {
        Class cls=objc_lookUpClass(name.UTF8String);
        if(!cls || !name.length || name.length>128 || [name rangeOfCharacterFromSet:classAllowed.invertedSet].location!=NSNotFound) continue;
        NSString *lower=name.lowercaseString;BOOL related=NO;
        if([lower containsString:@"keymaterial"]) continue;
        for(NSString *word in @[@"shader",@"scenekit",@"textur",@"material",@"metal",@"liverender",@"dynamiclayer"]) if([lower containsString:word]) related=YES;
        if(!related) continue;
        if(catalog.count>=16) { truncated=YES;continue; }
        unsigned int amount=0;Method *methods=class_copyMethodList(cls,&amount);
        NSMutableSet *names=[NSMutableSet set];BOOL methodsTruncated=NO;
        for(unsigned int j=0;j<amount;++j) {
            NSString *methodName=NSStringFromSelector(method_getName(methods[j]));NSString *methodLower=methodName.lowercaseString;
            if(!methodName.length || methodName.length>128 || [methodName rangeOfCharacterFromSet:allowed.invertedSet].location!=NSNotFound) continue;
            BOOL relevant=NO;
            for(NSString *word in @[@"shader",@"scene",@"textur",@"material",@"render",@"diffuse",@"layer",@"animation",@"image"]) if([methodLower containsString:word]) relevant=YES;
            if(!relevant) continue;
            if(names.count>=8 && ![names containsObject:methodName]) { methodsTruncated=YES;continue; }
            [names addObject:methodName];
        }
        free(methods);
        [catalog addObject:@{@"class_name":name,@"method_names":[[names allObjects] sortedArrayUsingSelector:@selector(compare:)],@"methods_truncated":@(methodsTruncated)}];
    }
    return @{@"classes":catalog,@"truncated":@(truncated)};
}

static NSDictionary *ProbeOwnTextureConstructor(void) {
    NSMutableArray *candidates=[NSMutableArray array];
    Class cls=OwnMaterialConstructorClass(candidates);
    Method method=class_getInstanceMethod(cls,NSSelectorFromString(@"initWithDiffuse:metalness:normal:"));
    BOOL valid=method && method_getNumberOfArguments(method)==5;
    if(valid) {
        char ret[128]={0};method_getReturnType(method,ret,sizeof(ret));valid=ret[0]=='@';
        for(unsigned int i=2;i<5 && valid;++i) {
            char arg[128]={0};method_getArgumentType(method,i,arg,sizeof(arg));
            valid=arg[0]=='^' && strstr(arg,"CGImage");
        }
    }
    NSMutableDictionary *result=[ProbeOwnCodeForMethod(valid?method:NULL) mutableCopy];
    unsigned int count=0; Method *methods=class_copyMethodList(cls,&count);
    NSMutableSet *names=[NSMutableSet set]; BOOL truncated=NO;
    NSCharacterSet *allowed=[NSCharacterSet characterSetWithCharactersInString:@"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_:"];
    for(unsigned int i=0;i<count;++i) {
        NSString *name=NSStringFromSelector(method_getName(methods[i]));
        if(!name.length || name.length>128 || [name rangeOfCharacterFromSet:allowed.invertedSet].location!=NSNotFound) continue;
        if(names.count>=128 && ![names containsObject:name]) { truncated=YES; continue; }
        [names addObject:name];
    }
    free(methods);
    result[@"method_names"]=[[names allObjects] sortedArrayUsingSelector:@selector(compare:)];
    result[@"inventory_truncated"]=@(truncated);
    result[@"class_name"]=cls?@(class_getName(cls)):@"";
    result[@"constructor_candidates"]=candidates;
    NSString *image=OwnTextureImage();NSString *leaf=image.lastPathComponent;
    NSCharacterSet *imageAllowed=[NSCharacterSet characterSetWithCharactersInString:@"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_."];
    result[@"class_image"]=(leaf.length && leaf.length<=128 && [leaf rangeOfCharacterFromSet:imageAllowed.invertedSet].location==NSNotFound)?leaf:@"";
    return result;
}
