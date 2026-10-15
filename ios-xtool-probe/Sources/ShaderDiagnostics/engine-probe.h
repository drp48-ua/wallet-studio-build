static NSDictionary *EmptyOwnEngineState(void) {
    return @{@"found":@NO,@"verified":@NO,@"paused":@NO,@"motion_enabled":@NO};
}

static NSDictionary *OwnEngineState(UIView *root, NSUInteger depth) {
    if(depth>8) return EmptyOwnEngineState();
    Class expected=NSClassFromString(@"PKTexturedCardView");
    if(expected && [root isKindOfClass:expected]) {
        Class concrete=object_getClass(root);
        SEL paused=NSSelectorFromString(@"isPaused"),motion=NSSelectorFromString(@"isMotionEnabled");
        if(!ScalarGetter(concrete,paused,@encode(BOOL)) || !ScalarGetter(concrete,motion,@encode(BOOL)))
            return @{@"found":@YES,@"verified":@NO,@"paused":@NO,@"motion_enabled":@NO};
        BOOL (*readPaused)(id,SEL)=(void*)class_getMethodImplementation(concrete,paused);
        BOOL (*readMotion)(id,SEL)=(void*)class_getMethodImplementation(concrete,motion);
        return @{@"found":@YES,@"verified":@YES,@"paused":@(readPaused(root,paused)),@"motion_enabled":@(readMotion(root,motion))};
    }
    for(UIView *child in root.subviews) {
        NSDictionary *state=OwnEngineState(child,depth+1);
        if([state[@"found"] boolValue]) return state;
    }
    return EmptyOwnEngineState();
}

static NSDictionary *OwnEngineMethods(Class cls) {
    NSMutableSet *names=[NSMutableSet set];BOOL truncated=NO;
    unsigned int count=0;Method *methods=class_copyMethodList(cls,&count);
    NSCharacterSet *allowed=[NSCharacterSet characterSetWithCharactersInString:@"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_:"];
    for(unsigned int i=0;i<count;++i) {
        NSString *name=NSStringFromSelector(method_getName(methods[i]));
        if(!name.length || name.length>128 || [name rangeOfCharacterFromSet:allowed.invertedSet].location!=NSNotFound) continue;
        if(names.count>=48 && ![names containsObject:name]) { truncated=YES;continue; }
        [names addObject:name];
    }
    free(methods);return @{@"names":[[names allObjects] sortedArrayUsingSelector:@selector(compare:)],@"truncated":@(truncated)};
}

static NSArray *OwnEngineInventory(void) {
    NSMutableArray *items=[NSMutableArray array];
    for(NSString *name in @[@"PKTexturedCardRenderer",@"PKTextureLoader",@"PKMetalRenderLoop"]) {
        Class cls=NSClassFromString(name);
        [items addObject:@{@"class_name":name,@"available":@((BOOL)(cls!=Nil)),
            @"instance_methods":OwnEngineMethods(cls),@"class_methods":OwnEngineMethods(object_getClass(cls))}];
    }
    return items;
}
