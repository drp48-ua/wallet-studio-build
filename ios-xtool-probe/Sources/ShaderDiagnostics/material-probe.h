@protocol NativeMaterialImageInitializing <NSObject>
- (instancetype)initWithData:(NSData *)data scale:(double)scale;
@end
static BOOL BindSyntheticDiffuse(id pass, NSMutableDictionary *result) {
    Class imageClass=NSClassFromString(@"PKImage");
    Class setClass=NSClassFromString(@"PKPassLiveRenderedImageSet");
    if (!imageClass || !setClass) return NO;
    Method init=class_getInstanceMethod(imageClass,@selector(initWithData:scale:));
    char ret[64]={0},dataType[64]={0},scaleType[64]={0};
    if(!init || method_getNumberOfArguments(init)!=4) return NO;
    method_getReturnType(init,ret,sizeof(ret)); method_getArgumentType(init,2,dataType,sizeof(dataType)); method_getArgumentType(init,3,scaleType,sizeof(scaleType));
    if(ret[0]!='@' || dataType[0]!='@' || strcmp(scaleType,@encode(double))) return NO;
    UIGraphicsImageRenderer *canvas=StandardSyntheticRenderer(CGSizeMake(320,202));
    UIImage *white=[canvas imageWithActions:^(UIGraphicsImageRendererContext *context){
        [UIColor.whiteColor setFill]; UIRectFill(CGRectMake(0,0,320,202));
    }];
    id image=[(id<NativeMaterialImageInitializing>)[imageClass alloc] initWithData:UIImagePNGRepresentation(white) scale:1.0];
    if(!image || ![image isKindOfClass:imageClass]) return NO;
    id set=[setClass new];
    SEL diffuse=NSSelectorFromString(@"setDiffuseMaterialPropertyImage:");
    Method diffuseMethod=class_getInstanceMethod(setClass,diffuse);
    char diffuseRet[64]={0},diffuseArg[64]={0};
    if(!set || !diffuseMethod || method_getNumberOfArguments(diffuseMethod)!=3) return NO;
    method_getReturnType(diffuseMethod,diffuseRet,sizeof(diffuseRet)); method_getArgumentType(diffuseMethod,2,diffuseArg,sizeof(diffuseArg));
    if(diffuseRet[0]!='v' || diffuseArg[0]!='@') return NO;
    void (*putDiffuse)(id,SEL,id)=(void*)method_getImplementation(diffuseMethod); putDiffuse(set,diffuse,image);
    SEL typeSelector=NSSelectorFromString(@"imageSetType");
    Method typeMethod=class_getClassMethod(setClass,typeSelector);
    char typeReturn[64]={0};
    if(!typeMethod || method_getNumberOfArguments(typeMethod)!=2) return NO;
    method_getReturnType(typeMethod,typeReturn,sizeof(typeReturn));
    if(strcmp(typeReturn,@encode(long long))) return NO;
    long long (*getType)(id,SEL)=(void*)method_getImplementation(typeMethod);
    long long type=getType(setClass,typeSelector);
    if(type<0 || type>1024) return NO;
    SEL bind=NSSelectorFromString(@"setImageSet:forImageSetType:");
    Method bindMethod=class_getInstanceMethod(object_getClass(pass),bind);
    char bindReturn[64]={0},first[64]={0},second[64]={0};
    if(!bindMethod || method_getNumberOfArguments(bindMethod)!=4) return NO;
    method_getReturnType(bindMethod,bindReturn,sizeof(bindReturn)); method_getArgumentType(bindMethod,2,first,sizeof(first)); method_getArgumentType(bindMethod,3,second,sizeof(second));
    if(bindReturn[0]!='v' || first[0]!='@' || strcmp(second,@encode(long long))) return NO;
    void (*bindSet)(id,SEL,id,long long)=(void*)method_getImplementation(bindMethod); bindSet(pass,bind,set,type);
    SEL getter=NSSelectorFromString(@"liveRenderedImageSet");
    if(!ObjectMethod(object_getClass(pass),getter,0)) return NO;
    id (*getSet)(id,SEL)=(void*)class_getMethodImplementation(object_getClass(pass),getter);
    BOOL exact=getSet(pass,getter)==set;
    result[@"material_bound"]=@(exact); result[@"native_image_set_type"]=@(type);
    return exact;
}
