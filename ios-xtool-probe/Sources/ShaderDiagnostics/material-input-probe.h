static BOOL CompleteOwnMaterial(id pass, NSMutableDictionary *result) {
    Class imageClass=NSClassFromString(@"PKImage"),setClass=NSClassFromString(@"PKPassLiveRenderedImageSet");
    SEL getSet=NSSelectorFromString(@"liveRenderedImageSet");
    if(!imageClass || !setClass || !ObjectMethod(object_getClass(pass),getSet,0)) return NO;
    id (*setGetter)(id,SEL)=(void*)class_getMethodImplementation(object_getClass(pass),getSet);
    id set=setGetter(pass,getSet);
    if(![set isKindOfClass:setClass]) return NO;
    SEL imageRefSelector=NSSelectorFromString(@"imageRef");
    Method imageRefMethod=class_getInstanceMethod(imageClass,imageRefSelector);
    char type[128]={0};
    if(!imageRefMethod || method_getNumberOfArguments(imageRefMethod)!=2) return NO;
    method_getReturnType(imageRefMethod,type,sizeof(type));
    if(type[0]!='^' || !strstr(type,"CGImage")) return NO;
    CGImageRef (*imageRef)(id,SEL)=(void*)method_getImplementation(imageRefMethod);
    NSUInteger count=0;
    for(NSString *name in @[@"diffuseMaterialPropertyImage",@"metalnessMaterialPropertyImage",@"normalMaterialPropertyImage"]) {
        SEL getter=NSSelectorFromString(name);
        if(!ObjectMethod(object_getClass(set),getter,0)) return NO;
        id (*get)(id,SEL)=(void*)class_getMethodImplementation(object_getClass(set),getter);
        id image=get(set,getter);
        if(![name isEqual:@"diffuseMaterialPropertyImage"]) {
            UIColor *color=[name isEqual:@"metalnessMaterialPropertyImage"]?UIColor.blackColor:
                [UIColor colorWithRed:0.5 green:0.5 blue:1.0 alpha:1.0];
            UIGraphicsImageRenderer *canvas=StandardSyntheticRenderer(CGSizeMake(320,202));
            UIImage *fixture=[canvas imageWithActions:^(UIGraphicsImageRendererContext *context){
                [color setFill]; UIRectFill(CGRectMake(0,0,320,202));
            }];
            image=[(id<NativeMaterialImageInitializing>)[imageClass alloc] initWithData:UIImagePNGRepresentation(fixture) scale:1.0];
            NSString *setterName=[NSString stringWithFormat:@"set%@%@:",[[name substringToIndex:1] uppercaseString],[name substringFromIndex:1]];
            SEL setter=NSSelectorFromString(setterName); Method method=class_getInstanceMethod(object_getClass(set),setter);
            char ret[64]={0},arg[64]={0};
            if(!method || method_getNumberOfArguments(method)!=3) return NO;
            method_getReturnType(method,ret,sizeof(ret)); method_getArgumentType(method,2,arg,sizeof(arg));
            if(ret[0]!='v' || arg[0]!='@') return NO;
            void (*put)(id,SEL,id)=(void*)method_getImplementation(method); put(set,setter,image);
            if(get(set,getter)!=image) return NO;
        }
        if(![image isKindOfClass:imageClass]) return NO;
        CGImageRef cg=imageRef(image,imageRefSelector);
        if(!cg || CGImageGetWidth(cg)==0 || CGImageGetHeight(cg)==0) return NO;
        ++count;
    }
    result[@"image_ref_count"]=@(count);
    BOOL exact=count==3 && setGetter(pass,getSet)==set;
    result[@"complete_material_bound"]=@(exact);
    return exact;
}
