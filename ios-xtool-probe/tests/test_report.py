import copy,unittest
from validate_report import validate

class ReportTests(unittest.TestCase):
    def setUp(self):
        native={'status':'observed','object_returned':True,'exception':False,'getter_lengths':[0,0,0,0]}
        self.report={'schema':1,'revision':'shader_resources_v1','scope':'synthetic_empty_accessor_only','Wallet_animation_proven':False,'runtime_iOS':'26.2','environment':'simulator','framework_loaded':True,'probe':{'status':'observed','reason':'native_resource_requests_observed','reference':native,'observer':copy.deepcopy(native),'requests':[{'method':'named_resource','name':'surface','extension':'shader'}],'calibration':True,'parity':True,'trace_rejected':False,'trace_truncated':False}}
        self.report['probe']['rejection_kinds']=[]
        self.report['revision']='shader_resources_v4'
        self.report['scope']='synthetic_accessors_and_own_files_only'
        self.report['file_loader']={'status':'unknown','reason':'discovery_unavailable','native_accessor_created':False,'source_preserved':False}
    def test_loader_false_success(self):
        self.report['file_loader']['source_preserved']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_v5_unknown(self):
        self.report['revision']='shader_resources_v5'
        self.report['pass_probe']={'status':'unknown','reason':'loader_control_unavailable','reference':{},'observer':{},'parity':False,'metadata_lookup_keys':[],'trace_rejected':False,'trace_truncated':False}
        validate(self.report)
    def test_v5_unknown_trace_rejected(self):
        self.report['revision']='shader_resources_v5'
        self.report['pass_probe']={'status':'unknown','reason':'loader_control_unavailable','reference':{},'observer':{},'parity':False,'metadata_lookup_keys':['liveRenderType'],'trace_rejected':False,'trace_truncated':False}
        with self.assertRaises(ValueError): validate(self.report)
    def v7_unknown(self):
        self.report['revision']='shader_resources_v7'
        self.report['pass_probe']={'status':'unknown','reason':'loader_control_unavailable','reference':{},'observer':{},'parity':False,'metadata_lookup_keys':[],'trace_rejected':False,'trace_truncated':False,'bound_reference':{},'bound_observer':{},'bound_parity':False,'live_reference':{},'live_observer':{},'live_parity':False}
        self.report['renderer_probe']={'status':'unknown','reason':'pass_control_unavailable','view_created':False,'shader_source_exact':False,'capture_control':False,'view_kinds':[],'samples_rgb':[],'three_phases_observed':False}
    def test_v7_unknown(self):
        self.v7_unknown(); validate(self.report)
    def v11_unknown(self):
        self.v7_unknown(); self.report['revision']='shader_resources_v11'
        self.report['renderer_probe'].update(synthetic_model='PKPaymentPass',material_bound=False,native_image_set_type=0,initializer_names=[],admission_probe={'status':'unknown','getter_names':[],'trace_truncated':False,'constructor_nil_parity':False})
    def test_v11_unknown(self):
        self.v11_unknown(); validate(self.report)
    def test_v11_private_trace_rejected(self):
        self.v11_unknown(); self.report['renderer_probe']['admission_probe']['getter_names']=['privateIdentity']
        with self.assertRaises(ValueError): validate(self.report)
    def test_v11_false_admission_rejected(self):
        self.v11_unknown(); self.report['renderer_probe']['admission_probe']['status']='observed'
        with self.assertRaises(ValueError): validate(self.report)
    def v12_unknown(self):
        self.v11_unknown(); self.report['revision']='shader_resources_v12'
        self.report['renderer_probe']['profile_probe']={'status':'unknown','reason':'profile_control_unavailable','reference_present':False,'constructed':False,'parity':False,'bound':False,'metadata_lookup_keys':[],'trace_rejected':False,'trace_truncated':False,'reference_snapshot':{},'observer_snapshot':{}}
    def test_v12_unknown(self):
        self.v12_unknown(); validate(self.report)
    def test_v12_false_binding_rejected(self):
        self.v12_unknown(); self.report['renderer_probe']['profile_probe']['bound']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_v12_false_parity_rejected(self):
        self.v12_unknown(); self.report['renderer_probe']['profile_probe']['parity']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_v12_private_profile_key_rejected(self):
        self.v12_unknown(); self.report['renderer_probe']['profile_probe']['metadata_lookup_keys']=['/private/card']
        with self.assertRaises(ValueError): validate(self.report)
    def v12_bound_profile(self):
        self.v12_unknown()
        snapshot={'type':0,'passStyle':0,'layoutMode':0,'backgroundColorPresent':True,'foregroundColorPresent':True,'labelColorPresent':True}
        self.report['renderer_probe']['profile_probe'].update(status='observed',reason='synthetic_profile_bound',constructed=True,parity=True,bound=True,reference_snapshot=snapshot,observer_snapshot=copy.deepcopy(snapshot),metadata_lookup_keys=['formatVersion'])
    def test_v12_bound_profile_requires_established_pass(self):
        self.v12_bound_profile()
        with self.assertRaises(ValueError): validate(self.report)
    def test_v12_profile_binding_can_succeed_without_wallet_animation(self):
        self.v12_bound_profile()
        self.report['file_loader'].update(status='observed',reason='synthetic_source_preserved',native_accessor_created=True,source_preserved=True)
        self.report['renderer_probe']['capture_control']=True
        self.report['pass_probe'].update(live_parity=True,live_reference={'status':'observed','pass_created':True,'exception':False,'live_background':True,'live_render_type':1,'shader_object_returned':True,'shader_source_preserved':True})
        validate(self.report)
        self.assertFalse(self.report['Wallet_animation_proven'])
    def v13_code(self):
        self.v12_unknown(); self.report['revision']='shader_resources_v13'
        self.report['constructor_probe']={'status':'observed','reason':'own_constructor_code_observed','image_origin_verified':True,'code_hex':'00'*512}
    def test_v13_bounded_code(self):
        self.v13_code(); validate(self.report)
    def test_v13_rejects_unverified_image(self):
        self.v13_code(); self.report['constructor_probe']['image_origin_verified']=False
        with self.assertRaises(ValueError): validate(self.report)
    def test_v13_rejects_excess_bytes(self):
        self.v13_code(); self.report['constructor_probe']['code_hex']+='00'
        with self.assertRaises(ValueError): validate(self.report)
    def test_v13_rejects_paths(self):
        self.v13_code(); self.report['constructor_probe']['code_hex']='/private/user/card'
        with self.assertRaises(ValueError): validate(self.report)
    def test_v13_rejects_false_observation(self):
        self.v13_code(); self.report['constructor_probe']['status']='unknown'
        with self.assertRaises(ValueError): validate(self.report)
    def test_v13_unknown(self):
        self.v13_code(); self.report['constructor_probe'].update(status='unknown',reason='method_unavailable',image_origin_verified=False,code_hex='')
        validate(self.report)
    def v14_code(self):
        self.v13_code(); self.report['revision']='shader_resources_v14'
        self.report['constructor_probe'].update(code_hex='00'*4096,selector_calls=[{'offset':4,'selector':'liveRenderType'}])
    def test_v14_bounded_code(self):
        self.v14_code(); validate(self.report)
    def test_v14_rejects_call_path(self):
        self.v14_code(); self.report['constructor_probe']['selector_calls'][0]['selector']='/private/card'
        with self.assertRaises(ValueError): validate(self.report)
    def test_v14_rejects_duplicate_offset(self):
        self.v14_code(); self.report['constructor_probe']['selector_calls']*=2
        with self.assertRaises(ValueError): validate(self.report)
    def test_v14_rejects_excess_code(self):
        self.v14_code(); self.report['constructor_probe']['code_hex']+='00'
        with self.assertRaises(ValueError): validate(self.report)
    def v15_material(self):
        self.v14_code(); self.report['revision']='shader_resources_v15'
        self.report['renderer_probe']['material_inputs']={'baseline_view_nil':False,'complete_material_bound':False,'image_ref_count':0,'candidate_view_created':False}
    def test_v15_unknown(self):
        self.v15_material(); validate(self.report)
    def test_v15_rejects_false_material(self):
        self.v15_material(); self.report['renderer_probe']['material_inputs']['complete_material_bound']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_v15_rejects_false_view(self):
        self.v15_material(); self.report['renderer_probe']['material_inputs']['candidate_view_created']=True
        with self.assertRaises(ValueError): validate(self.report)
    def v16_capture(self):
        self.v15_material(); self.report['revision']='shader_resources_v16'
        self.report['renderer_probe'].update(capture_control_rgb=[],capture_state={'application_active':True,'host_in_window':True})
    def test_v16_unknown(self):
        self.v16_capture(); validate(self.report)
    def test_v16_false_red_rejected(self):
        self.v16_capture(); self.report['renderer_probe'].update(capture_control=True,capture_control_rgb=[0,0,0])
        with self.assertRaises(ValueError): validate(self.report)
    def test_v16_unsafe_rgb_rejected(self):
        self.v16_capture(); self.report['renderer_probe']['capture_control_rgb']=[255,0,999]
        with self.assertRaises(ValueError): validate(self.report)
    def v17_capture(self):
        self.v16_capture(); self.report['revision']='shader_resources_v17'
        self.report['renderer_probe']['capture_control_alpha']=0
    def test_v17_unknown(self):
        self.v17_capture(); validate(self.report)
    def test_v17_nonopaque_rejected(self):
        self.v17_capture(); self.report['renderer_probe'].update(capture_control=True,capture_control_rgb=[255,0,0],capture_control_alpha=163)
        with self.assertRaises(ValueError): validate(self.report)
    def test_v17_valid_red(self):
        self.v17_capture(); self.report['renderer_probe'].update(capture_control=True,capture_control_rgb=[255,0,0],capture_control_alpha=255)
        validate(self.report)
    def v18_texture(self):
        self.v17_capture(); self.report['revision']='shader_resources_v18'
        self.report['texture_probe']=copy.deepcopy(self.report['constructor_probe'])
        self.report['texture_probe'].update(method_names=['initWithDiffuse:metalness:normal:'],inventory_truncated=False)
    def test_v18_texture(self):
        self.v18_texture(); validate(self.report)
    def test_v18_private_method_path_rejected(self):
        self.v18_texture(); self.report['texture_probe']['method_names']=['/private/user/card']
        with self.assertRaises(ValueError): validate(self.report)
    def test_v18_duplicate_method_rejected(self):
        self.v18_texture(); self.report['texture_probe']['method_names']*=2
        with self.assertRaises(ValueError): validate(self.report)
    def v19_catalog(self):
        self.v18_texture(); self.report['revision']='shader_resources_v19'
        self.report['texture_probe']['class_name']='PKSyntheticMaterial'
        self.report['renderer_catalog']={'classes':[{'class_name':'PKSyntheticMaterial','method_names':['render'],'methods_truncated':False}],'truncated':False}
    def test_v19_catalog(self):
        self.v19_catalog(); validate(self.report)
    def test_v19_catalog_private_path_rejected(self):
        self.v19_catalog(); self.report['renderer_catalog']['classes'][0]['class_name']='/private/user/card'
        with self.assertRaises(ValueError): validate(self.report)
    def test_v19_catalog_duplicate_class_rejected(self):
        self.v19_catalog(); self.report['renderer_catalog']['classes']*=2
        with self.assertRaises(ValueError): validate(self.report)
    def test_v20_catalog(self):
        self.v19_catalog(); self.report['revision']='shader_resources_v20'
        self.report['texture_probe']['class_name']='PassKitUI.SyntheticMaterial'
        validate(self.report)
    def test_v20_catalog_remains_bounded(self):
        self.v19_catalog(); self.report['revision']='shader_resources_v20'
        self.report['renderer_catalog']['classes']*=17
        with self.assertRaises(ValueError): validate(self.report)
    def v21_catalog(self):
        self.v19_catalog(); self.report['revision']='shader_resources_v21'
        self.report['texture_probe'].update(constructor_candidates=[],class_image='SyntheticRenderer')
    def test_v21_catalog(self):
        self.v21_catalog(); validate(self.report)
    def test_v21_image_path_rejected(self):
        self.v21_catalog(); self.report['texture_probe']['class_image']='/private/user/card'
        with self.assertRaises(ValueError): validate(self.report)
    def v22_engine(self):
        self.v21_catalog(); self.report['revision']='shader_resources_v22'
        self.report['engine_inventory']=[{'class_name':name,'available':False,'instance_methods':{'names':[],'truncated':False},'class_methods':{'names':[],'truncated':False}} for name in ('PKTexturedCardRenderer','PKTextureLoader','PKMetalRenderLoop')]
        for key in ('engine_state_before','engine_state_after'):self.report['renderer_probe'][key]={'found':False,'verified':False,'paused':False,'motion_enabled':False}
    def test_v22_engine(self):
        self.v22_engine(); validate(self.report)
    def test_v22_unverified_state_rejected(self):
        self.v22_engine(); self.report['renderer_probe']['engine_state_after']['paused']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_v22_engine_private_method_rejected(self):
        self.v22_engine(); self.report['engine_inventory'][0]['instance_methods']['names']=['/private/card']
        with self.assertRaises(ValueError): validate(self.report)
    def test_v7_false_color_claim(self):
        self.v7_unknown(); self.report['renderer_probe']['three_phases_observed']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_v7_private_view_kind(self):
        self.v7_unknown(); self.report['renderer_probe']['view_kinds']=['private_device_identity']
        with self.assertRaises(ValueError): validate(self.report)
    def test_valid(self): validate(self.report)
    def test_private_key(self):
        self.report['UDID']='secret'
        with self.assertRaises(ValueError): validate(self.report)
    def test_wallet_claim(self):
        self.report['Wallet_animation_proven']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_path(self):
        self.report['probe']['requests'][0]['name']='/var/mobile/private'
        with self.assertRaises(ValueError): validate(self.report)
    def test_identity(self):
        self.report['probe']['requests'][0]['name']='card_'+ 'a'*40
        with self.assertRaises(ValueError): validate(self.report)
    def test_truncated(self):
        self.report['probe']['trace_truncated']=True
        with self.assertRaises(ValueError): validate(self.report)
    def test_wrong_parity(self):
        self.report['probe']['observer']['getter_lengths']=[1,0,0,0]
        with self.assertRaises(ValueError): validate(self.report)
    def test_unknown_trace(self):
        self.report['probe']['status']='unknown'
        with self.assertRaises(ValueError): validate(self.report)
    def test_missing_calibration(self):
        self.report['probe']['calibration']=False
        with self.assertRaises(ValueError): validate(self.report)
    def test_unknown(self):
        self.report['probe'].update(status='unknown',reason='runtime_unavailable',reference={},observer={},requests=[])
        validate(self.report)
    def test_exception_not_positive(self):
        self.report['probe']['reference']['exception']=True
        with self.assertRaises(ValueError): validate(self.report)

if __name__=='__main__': unittest.main()
