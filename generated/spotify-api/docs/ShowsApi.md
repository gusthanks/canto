# \ShowsApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**check_users_saved_shows**](ShowsApi.md#check_users_saved_shows) | **GET** /me/shows/contains | Check User's Saved Shows 
[**get_a_show**](ShowsApi.md#get_a_show) | **GET** /shows/{id} | Get Show 
[**get_a_shows_episodes**](ShowsApi.md#get_a_shows_episodes) | **GET** /shows/{id}/episodes | Get Show Episodes 
[**get_multiple_shows**](ShowsApi.md#get_multiple_shows) | **GET** /shows | Get Several Shows 
[**get_users_saved_shows**](ShowsApi.md#get_users_saved_shows) | **GET** /me/shows | Get User's Saved Shows 
[**remove_shows_user**](ShowsApi.md#remove_shows_user) | **DELETE** /me/shows | Remove User's Saved Shows 
[**save_shows_user**](ShowsApi.md#save_shows_user) | **PUT** /me/shows | Save Shows for Current User 



## check_users_saved_shows

> Vec<bool> check_users_saved_shows(ids)
Check User's Saved Shows 

Check if one or more shows is already saved in the current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_a_show

> models::ShowObject get_a_show(id, market)
Get Show 

Get Spotify catalog information for a single show identified by its unique Spotify ID. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::ShowObject**](ShowObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_a_shows_episodes

> models::PagingSimplifiedEpisodeObject get_a_shows_episodes(id, market, limit, offset)
Get Show Episodes 

Get Spotify catalog information about an show’s episodes. Optional parameters can be used to limit the number of episodes returned. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSimplifiedEpisodeObject**](PagingSimplifiedEpisodeObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_multiple_shows

> models::GetMultipleShows200Response get_multiple_shows(ids, market)
Get Several Shows 

Get Spotify catalog information for several shows based on their Spotify IDs. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::GetMultipleShows200Response**](get_multiple_shows_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_shows

> models::PagingSavedShowObject get_users_saved_shows(limit, offset)
Get User's Saved Shows 

Get a list of shows saved in the current Spotify user's library. Optional parameters can be used to limit the number of shows returned. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSavedShowObject**](PagingSavedShowObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_shows_user

> remove_shows_user(ids, market)
Remove User's Saved Shows 

Delete one or more shows from current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_shows_user

> save_shows_user(ids)
Save Shows for Current User 

Save one or more shows to current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

